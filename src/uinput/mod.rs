use crate::config::profiles::{supported_pad_keys, Action};
use crate::config::transform::{
    apply_precision_mode, logical_axis_bounds, rotated_normalized_coords,
    transform_absolute_coords, PressureCurveLut, RelativeAccumulator, ScreenMapping,
    TransformedStylusCoords,
};
use crate::config::DriverConfig;
use crate::error::DriverResult;
use crate::hid::{DiscoveredTablet, ExpressKeyEvent, PenEvent, TabletPacket};
use crate::ipc::LiveTelemetry;
use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{
    AbsInfo, AbsoluteAxisType, AttributeSet, BusType, EventType, InputEvent, InputId, Key,
    PropType, RelativeAxisType, UinputAbsSetup,
};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tracing::{debug, info, warn};

/// Abstração da saída de eventos `evdev`, permitindo testes unitários determinísticos sem `/dev/uinput`.
pub trait EventSink {
    fn emit_stylus(&mut self, events: &[InputEvent]) -> DriverResult<()>;
    fn emit_pad(&mut self, events: &[InputEvent]) -> DriverResult<()>;
    fn emit_native_pad(&mut self, events: &[InputEvent]) -> DriverResult<()>;
}

/// Dispositivo virtual `uinput` emulando a caneta (`Stylus`) no subsistema `evdev` Linux.
pub struct VirtualStylus {
    device: VirtualDevice,
    out_max_x: u32,
    out_max_y: u32,
    max_pressure: u16,
}

impl VirtualStylus {
    /// Cria o dispositivo virtual `/dev/uinput` da caneta com eixos e resolução física corretos.
    pub fn create(
        vid: u16,
        pid: u16,
        out_max_x: u32,
        out_max_y: u32,
        max_pressure: u16,
        lpi: u16,
        is_pen_display: bool,
    ) -> DriverResult<Self> {
        let mut keys = AttributeSet::<Key>::new();
        keys.insert(Key::BTN_TOOL_PEN);
        keys.insert(Key::BTN_TOOL_RUBBER);
        keys.insert(Key::BTN_TOUCH);
        keys.insert(Key::BTN_STYLUS);
        keys.insert(Key::BTN_STYLUS2);

        let mut props = AttributeSet::<PropType>::new();
        if is_pen_display {
            props.insert(PropType::DIRECT);
        } else {
            props.insert(PropType::POINTER);
        }

        // Kernel `input_absinfo.resolution` usa unidades/mm (LPI / 25.4).
        let resolution_per_mm = ((f64::from(lpi.max(1))) / 25.4).round() as i32;
        let abs_x = UinputAbsSetup::new(
            AbsoluteAxisType::ABS_X,
            AbsInfo::new(0, 0, out_max_x as i32, 0, 0, resolution_per_mm),
        );
        let abs_y = UinputAbsSetup::new(
            AbsoluteAxisType::ABS_Y,
            AbsInfo::new(0, 0, out_max_y as i32, 0, 0, resolution_per_mm),
        );
        let abs_pressure = UinputAbsSetup::new(
            AbsoluteAxisType::ABS_PRESSURE,
            AbsInfo::new(0, 0, i32::from(max_pressure), 0, 0, 0),
        );
        let abs_tilt_x = UinputAbsSetup::new(
            AbsoluteAxisType::ABS_TILT_X,
            AbsInfo::new(0, -60, 60, 0, 0, 0),
        );
        let abs_tilt_y = UinputAbsSetup::new(
            AbsoluteAxisType::ABS_TILT_Y,
            AbsInfo::new(0, -60, 60, 0, 0, 0),
        );

        let device = VirtualDeviceBuilder::new()?
            .name("UGEE Tablet Stylus")
            .input_id(InputId::new(BusType::BUS_USB, vid, pid, 1))
            .with_properties(&props)?
            .with_keys(&keys)?
            .with_absolute_axis(&abs_x)?
            .with_absolute_axis(&abs_y)?
            .with_absolute_axis(&abs_pressure)?
            .with_absolute_axis(&abs_tilt_x)?
            .with_absolute_axis(&abs_tilt_y)?
            .build()?;

        info!(
            out_max_x,
            out_max_y,
            max_pressure,
            resolution_per_mm,
            is_pen_display,
            "dispositivo virtual uinput Stylus criado"
        );

        Ok(Self {
            device,
            out_max_x,
            out_max_y,
            max_pressure,
        })
    }

    pub fn bounds(&self) -> (u32, u32, u16) {
        (self.out_max_x, self.out_max_y, self.max_pressure)
    }

    fn emit(&mut self, events: &[InputEvent]) -> DriverResult<()> {
        self.device.emit(events)?;
        Ok(())
    }
}

/// Dispositivo virtual `uinput` auxiliar para Express Keys, Touch Ring, Mouse e Modo Relativo.
pub struct VirtualPad {
    device: VirtualDevice,
}

impl VirtualPad {
    /// Cria o dispositivo virtual de teclado/mouse auxiliar.
    pub fn create(vid: u16, pid: u16, extra_actions: &[&Action]) -> DriverResult<Self> {
        let mut keys = AttributeSet::<Key>::new();
        for key in supported_pad_keys() {
            keys.insert(key);
        }
        for action in extra_actions {
            if let Action::KeyCombo { keys: combo } = action {
                for &code in combo {
                    keys.insert(Key::new(code));
                }
            }
        }

        let mut rel_axes = AttributeSet::<RelativeAxisType>::new();
        rel_axes.insert(RelativeAxisType::REL_X);
        rel_axes.insert(RelativeAxisType::REL_Y);
        rel_axes.insert(RelativeAxisType::REL_WHEEL);
        rel_axes.insert(RelativeAxisType::REL_HWHEEL);

        let device = VirtualDeviceBuilder::new()?
            .name("UGEE Tablet ExpressKeys")
            .input_id(InputId::new(BusType::BUS_USB, vid, pid, 1))
            .with_keys(&keys)?
            .with_relative_axes(&rel_axes)?
            .build()?;

        info!("dispositivo virtual uinput ExpressKeys criado");
        Ok(Self { device })
    }

    fn emit(&mut self, events: &[InputEvent]) -> DriverResult<()> {
        self.device.emit(events)?;
        Ok(())
    }
}

/// Dispositivo virtual `uinput` nativo para botões `BTN_0..BTN_7` (`ID_INPUT_TABLET_PAD`).
pub struct VirtualNativePad {
    device: VirtualDevice,
}

impl VirtualNativePad {
    /// Cria o dispositivo virtual `UGEE Tablet Pad` separado do teclado/mouse de atalhos.
    pub fn create(vid: u16, pid: u16) -> DriverResult<Self> {
        let mut keys = AttributeSet::<Key>::new();
        for btn in [
            Key::BTN_0,
            Key::BTN_1,
            Key::BTN_2,
            Key::BTN_3,
            Key::BTN_4,
            Key::BTN_5,
            Key::BTN_6,
            Key::BTN_7,
        ] {
            keys.insert(btn);
        }

        let abs_misc = UinputAbsSetup::new(
            AbsoluteAxisType::ABS_MISC,
            AbsInfo::new(0, 0, 0, 0, 0, 0),
        );

        let device = VirtualDeviceBuilder::new()?
            .name("UGEE Tablet Pad")
            .input_id(InputId::new(BusType::BUS_USB, vid, pid, 1))
            .with_keys(&keys)?
            .with_absolute_axis(&abs_misc)?
            .build()?;

        info!("dispositivo virtual uinput Tablet Pad criado");
        Ok(Self { device })
    }

    fn emit(&mut self, events: &[InputEvent]) -> DriverResult<()> {
        self.device.emit(events)?;
        Ok(())
    }
}

/// Sink real baseado nos dispositivos virtuais `/dev/uinput`.
pub struct UinputSink {
    stylus: VirtualStylus,
    pad: VirtualPad,
    native_pad: VirtualNativePad,
}

impl EventSink for UinputSink {
    fn emit_stylus(&mut self, events: &[InputEvent]) -> DriverResult<()> {
        self.stylus.emit(events)
    }

    fn emit_pad(&mut self, events: &[InputEvent]) -> DriverResult<()> {
        self.pad.emit(events)
    }

    fn emit_native_pad(&mut self, events: &[InputEvent]) -> DriverResult<()> {
        self.native_pad.emit(events)
    }
}

/// Motor genérico de tradução de pacotes HID para eventos `evdev`.
pub struct TabletDispatcherCore<S: EventSink> {
    config: DriverConfig,
    phys_max_x: u32,
    phys_max_y: u32,
    out_max_x: u32,
    out_max_y: u32,
    max_pressure: u16,
    lpi: u16,
    pressure_lut: PressureCurveLut,
    sink: S,
    rel_accum: RelativeAccumulator,
    rel_active: bool,
    prev_rel_left: bool,
    prev_keys_mask: u64,
    prev_barrel_1: bool,
    prev_barrel_2: bool,
    eraser_toggled: bool,
    in_proximity: bool,
    active_rubber_tool: bool,
    precision_mode: bool,
    brush_eraser_use_e: bool,
    active_brush_eraser_key: Option<Key>,
    ring_1_mode_idx: usize,
    last_telemetry: LiveTelemetry,
}

/// Dispatcher padrão conectado ao subsistema `/dev/uinput` do kernel Linux.
pub type TabletDispatcher = TabletDispatcherCore<UinputSink>;

impl TabletDispatcher {
    /// Inicializa os dispositivos virtuais `uinput` conforme os descritores físicos da mesa e config.
    pub fn new(tablet: &DiscoveredTablet, config: DriverConfig) -> DriverResult<Self> {
        let (phys_max_x, phys_max_y, max_pressure, lpi) = tablet.physical_bounds();
        let (out_max_x, out_max_y) =
            logical_axis_bounds(phys_max_x, phys_max_y, config.orientation);

        let extra_actions: Vec<&Action> = config
            .express_keys
            .values()
            .chain([
                &config.pen_buttons.barrel_1,
                &config.pen_buttons.barrel_2,
                &config.ring_1.cw,
                &config.ring_1.ccw,
                &config.ring_2.cw,
                &config.ring_2.ccw,
            ])
            .chain(config.ring_1_modes.iter().flat_map(|r| [&r.cw, &r.ccw]))
            .chain(config.ring_2_modes.iter().flat_map(|r| [&r.cw, &r.ccw]))
            .collect();

        let stylus = VirtualStylus::create(
            tablet.vid,
            tablet.pid,
            out_max_x,
            out_max_y,
            max_pressure,
            lpi,
            tablet.profile.has_calibration,
        )?;
        let pad = VirtualPad::create(tablet.vid, tablet.pid, &extra_actions)?;
        let native_pad = VirtualNativePad::create(tablet.vid, tablet.pid)?;

        Ok(Self::with_sink(
            UinputSink {
                stylus,
                pad,
                native_pad,
            },
            phys_max_x,
            phys_max_y,
            max_pressure,
            lpi,
            config,
        ))
    }
}

impl<S: EventSink> TabletDispatcherCore<S> {
    /// Constrói o dispatcher sobre qualquer `EventSink` (real ou mock em testes).
    pub fn with_sink(
        sink: S,
        phys_max_x: u32,
        phys_max_y: u32,
        max_pressure: u16,
        lpi: u16,
        config: DriverConfig,
    ) -> Self {
        let (out_max_x, out_max_y) =
            logical_axis_bounds(phys_max_x, phys_max_y, config.orientation);
        let pressure_lut = config.build_pressure_lut(max_pressure);
        let precision_mode = config.precision_mode;

        Self {
            config,
            phys_max_x,
            phys_max_y,
            out_max_x,
            out_max_y,
            max_pressure,
            lpi,
            pressure_lut,
            sink,
            rel_accum: RelativeAccumulator::default(),
            rel_active: false,
            prev_rel_left: false,
            prev_keys_mask: 0,
            prev_barrel_1: false,
            prev_barrel_2: false,
            eraser_toggled: false,
            in_proximity: false,
            active_rubber_tool: false,
            precision_mode,
            brush_eraser_use_e: false,
            active_brush_eraser_key: None,
            ring_1_mode_idx: 0,
            last_telemetry: LiveTelemetry {
                precision_mode,
                ..LiveTelemetry::default()
            },
        }
    }

    /// Retorna os limites lógicos `(out_max_x, out_max_y, max_pressure)` registrados na caneta.
    pub fn stylus_bounds(&self) -> (u32, u32, u16) {
        (self.out_max_x, self.out_max_y, self.max_pressure)
    }

    /// Acesso somente leitura ao sink (útil em testes unitários).
    pub fn sink(&self) -> &S {
        &self.sink
    }

    /// Configuração atualmente aplicada no dispatcher.
    pub fn config(&self) -> &DriverConfig {
        &self.config
    }

    /// Última telemetria decodificada/mapeada pela instância do dispatcher.
    pub fn telemetry(&self) -> &LiveTelemetry {
        &self.last_telemetry
    }

    /// Indica se a troca de configuração exige recriar os dispositivos `uinput` no kernel
    /// (por exemplo, mudança entre orientação paisagem e retrato).
    pub fn needs_device_recreate(&self, next_config: &DriverConfig) -> bool {
        let next_bounds =
            logical_axis_bounds(self.phys_max_x, self.phys_max_y, next_config.orientation);
        next_bounds != (self.out_max_x, self.out_max_y)
    }

    /// Atualiza parâmetros dinâmicos da configuração (curva de pressão, área ativa, botões, etc.)
    /// sem recriar os dispositivos `uinput`, liberando antes quaisquer teclas/botões pressionados.
    pub fn update_config(&mut self, config: DriverConfig) {
        if let Err(err) = self.release_active_mappings() {
            warn!(error = %err, "erro ao liberar botões ativos antes de atualizar configuração");
        }
        self.pressure_lut = config.build_pressure_lut(self.max_pressure);
        self.rel_accum.reset();
        self.precision_mode = config.precision_mode;
        self.last_telemetry.precision_mode = self.precision_mode;
        self.config = config;
    }

    fn release_active_pen_buttons(&mut self) -> DriverResult<()> {
        if self.prev_barrel_1 {
            let action = self.config.pen_buttons.barrel_1.clone();
            self.apply_button_action(&action, false)?;
            self.prev_barrel_1 = false;
        }
        if self.prev_barrel_2 {
            let action = self.config.pen_buttons.barrel_2.clone();
            self.apply_button_action(&action, false)?;
            self.prev_barrel_2 = false;
        }
        Ok(())
    }

    fn release_active_express_keys(&mut self) -> DriverResult<()> {
        if self.prev_keys_mask != 0 {
            let active_mask = self.prev_keys_mask;
            self.prev_keys_mask = 0;
            for bit in 0..40usize {
                if let Some(mask) = 1u64.checked_shl(bit as u32) {
                    if (active_mask & mask) != 0 {
                        if let Some(action) = self.config.express_key_action(bit).cloned() {
                            self.apply_button_action(&action, false)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn release_active_mappings(&mut self) -> DriverResult<()> {
        self.release_relative_state()?;
        self.release_active_pen_buttons()?;
        self.release_active_express_keys()?;

        if let Some(key) = self.active_brush_eraser_key.take() {
            self.sink
                .emit_pad(&[InputEvent::new(EventType::KEY, key.code(), 0)])?;
        }
        Ok(())
    }

    /// Processa um pacote HID decodificado e emite os eventos `evdev` correspondentes.
    pub fn handle_packet(&mut self, packet: TabletPacket) -> DriverResult<()> {
        match packet {
            TabletPacket::Pen(pen) => self.handle_pen(pen),
            TabletPacket::ExpressKey(keys) => self.handle_express_keys(keys),
            TabletPacket::OutOfRange { .. } => self.handle_out_of_range(),
            TabletPacket::System(_) | TabletPacket::Unknown { .. } => Ok(()),
        }
    }

    fn handle_pen(&mut self, pen: PenEvent) -> DriverResult<()> {
        if pen.relative_hint
            && pen.x == 0
            && pen.y == 0
            && !pen.tip_switch
            && !pen.barrel_1
            && !pen.barrel_2
        {
            return self.handle_out_of_range();
        }

        let mapped_pressure = self.pressure_lut.map(pen.pressure);
        let (norm_x, norm_y) = rotated_normalized_coords(
            pen.x,
            pen.y,
            self.phys_max_x,
            self.phys_max_y,
            self.config.orientation,
        );
        self.last_telemetry.in_range = true;
        self.last_telemetry.tip_switch = pen.tip_switch;
        self.last_telemetry.barrel_1 = pen.barrel_1;
        self.last_telemetry.barrel_2 = pen.barrel_2;
        self.last_telemetry.eraser = pen.eraser || self.eraser_toggled;
        self.last_telemetry.raw_x = pen.x;
        self.last_telemetry.raw_y = pen.y;
        self.last_telemetry.norm_x = norm_x;
        self.last_telemetry.norm_y = norm_y;
        self.last_telemetry.precision_mode = self.precision_mode;
        self.last_telemetry.raw_pressure = pen.pressure;
        self.last_telemetry.mapped_pressure = mapped_pressure;
        self.last_telemetry.tilt_x = if self.config.disable_tilt {
            0
        } else {
            pen.tilt_x
        };
        self.last_telemetry.tilt_y = if self.config.disable_tilt {
            0
        } else {
            pen.tilt_y
        };

        if self.config.relative_mode || pen.relative_hint {
            self.last_telemetry.inside_area = true;
            self.release_stylus_proximity()?;
            self.sync_pen_buttons(pen.barrel_1, pen.barrel_2)?;

            let (dx, dy) =
                self.rel_accum
                    .update(pen.x as i32, pen.y as i32, self.config.mouse_speed);
            self.last_telemetry.mapped_x = dx;
            self.last_telemetry.mapped_y = dy;
            let mut events = Vec::new();
            if dx != 0 {
                events.push(InputEvent::new(
                    EventType::RELATIVE,
                    RelativeAxisType::REL_X.0,
                    dx,
                ));
            }
            if dy != 0 {
                events.push(InputEvent::new(
                    EventType::RELATIVE,
                    RelativeAxisType::REL_Y.0,
                    dy,
                ));
            }
            if pen.tip_switch != self.prev_rel_left {
                events.push(InputEvent::new(
                    EventType::KEY,
                    Key::BTN_LEFT.code(),
                    i32::from(pen.tip_switch),
                ));
                self.prev_rel_left = pen.tip_switch;
            }
            if !events.is_empty() {
                self.sink.emit_pad(&events)?;
            }
            self.rel_active = true;
            return Ok(());
        }

        self.release_relative_state()?;

        let screen_mapping = if self.precision_mode {
            ScreenMapping::default()
        } else {
            self.config.screen_mapping
        };
        let transformed = transform_absolute_coords(
            pen.x,
            pen.y,
            pen.tilt_x,
            pen.tilt_y,
            self.phys_max_x,
            self.phys_max_y,
            self.lpi,
            self.config.orientation,
            self.config.calibration,
            self.config.active_area,
            screen_mapping,
            self.config.disable_tilt,
        );

        let TransformedStylusCoords::Absolute {
            mut x,
            mut y,
            tilt_x,
            tilt_y,
        } = transformed
        else {
            self.last_telemetry.inside_area = false;
            self.last_telemetry.mapped_pressure = 0;
            self.last_telemetry.tip_switch = false;
            self.release_relative_state()?;
            self.release_active_pen_buttons()?;
            return self.release_stylus_proximity();
        };

        if self.precision_mode {
            (x, y) = apply_precision_mode(x, y, self.out_max_x, self.out_max_y);
        }
        self.last_telemetry.inside_area = true;

        self.last_telemetry.mapped_x = x;
        self.last_telemetry.mapped_y = y;
        self.sync_pen_buttons(pen.barrel_1, pen.barrel_2)?;

        let is_rubber = pen.eraser || self.eraser_toggled;
        self.last_telemetry.eraser = is_rubber;
        let mut events = Vec::with_capacity(10);

        if !self.in_proximity || self.active_rubber_tool != is_rubber {
            if self.in_proximity && self.active_rubber_tool != is_rubber {
                let prev_tool = if self.active_rubber_tool {
                    Key::BTN_TOOL_RUBBER
                } else {
                    Key::BTN_TOOL_PEN
                };
                events.push(InputEvent::new(EventType::KEY, prev_tool.code(), 0));
            }
            let next_tool = if is_rubber {
                Key::BTN_TOOL_RUBBER
            } else {
                Key::BTN_TOOL_PEN
            };
            events.push(InputEvent::new(EventType::KEY, next_tool.code(), 1));
            self.in_proximity = true;
            self.active_rubber_tool = is_rubber;
        }

        events.push(InputEvent::new(
            EventType::ABSOLUTE,
            AbsoluteAxisType::ABS_X.0,
            x,
        ));
        events.push(InputEvent::new(
            EventType::ABSOLUTE,
            AbsoluteAxisType::ABS_Y.0,
            y,
        ));
        events.push(InputEvent::new(
            EventType::ABSOLUTE,
            AbsoluteAxisType::ABS_PRESSURE.0,
            i32::from(mapped_pressure),
        ));
        events.push(InputEvent::new(
            EventType::ABSOLUTE,
            AbsoluteAxisType::ABS_TILT_X.0,
            tilt_x,
        ));
        events.push(InputEvent::new(
            EventType::ABSOLUTE,
            AbsoluteAxisType::ABS_TILT_Y.0,
            tilt_y,
        ));
        events.push(InputEvent::new(
            EventType::KEY,
            Key::BTN_TOUCH.code(),
            i32::from(pen.tip_switch),
        ));
        if matches!(self.config.pen_buttons.barrel_1, Action::None) {
            events.push(InputEvent::new(
                EventType::KEY,
                Key::BTN_STYLUS.code(),
                i32::from(pen.barrel_1),
            ));
        }
        if matches!(self.config.pen_buttons.barrel_2, Action::None) {
            events.push(InputEvent::new(
                EventType::KEY,
                Key::BTN_STYLUS2.code(),
                i32::from(pen.barrel_2),
            ));
        }

        self.sink.emit_stylus(&events)
    }

    fn sync_pen_buttons(&mut self, barrel_1: bool, barrel_2: bool) -> DriverResult<()> {
        if barrel_1 != self.prev_barrel_1 {
            let action = self.config.pen_buttons.barrel_1.clone();
            self.apply_button_action(&action, barrel_1)?;
            self.prev_barrel_1 = barrel_1;
        }
        if barrel_2 != self.prev_barrel_2 {
            let action = self.config.pen_buttons.barrel_2.clone();
            self.apply_button_action(&action, barrel_2)?;
            self.prev_barrel_2 = barrel_2;
        }
        Ok(())
    }

    fn release_relative_state(&mut self) -> DriverResult<()> {
        if self.rel_active || self.prev_rel_left {
            self.rel_accum.reset();
            if self.prev_rel_left {
                self.sink
                    .emit_pad(&[InputEvent::new(EventType::KEY, Key::BTN_LEFT.code(), 0)])?;
                self.prev_rel_left = false;
            }
            self.rel_active = false;
        }
        Ok(())
    }

    fn release_stylus_proximity(&mut self) -> DriverResult<()> {
        if self.in_proximity {
            let tool = if self.active_rubber_tool {
                Key::BTN_TOOL_RUBBER
            } else {
                Key::BTN_TOOL_PEN
            };
            let events = [
                InputEvent::new(EventType::KEY, Key::BTN_TOUCH.code(), 0),
                InputEvent::new(EventType::KEY, Key::BTN_STYLUS.code(), 0),
                InputEvent::new(EventType::KEY, Key::BTN_STYLUS2.code(), 0),
                InputEvent::new(EventType::ABSOLUTE, AbsoluteAxisType::ABS_PRESSURE.0, 0),
                InputEvent::new(EventType::KEY, tool.code(), 0),
            ];
            self.sink.emit_stylus(&events)?;
            self.in_proximity = false;
        }
        Ok(())
    }

    fn handle_out_of_range(&mut self) -> DriverResult<()> {
        self.last_telemetry.in_range = false;
        self.last_telemetry.tip_switch = false;
        self.last_telemetry.barrel_1 = false;
        self.last_telemetry.barrel_2 = false;
        self.last_telemetry.raw_pressure = 0;
        self.last_telemetry.mapped_pressure = 0;

        self.release_relative_state()?;
        self.release_active_pen_buttons()?;

        self.release_stylus_proximity()
    }

    fn handle_express_keys(&mut self, ev: ExpressKeyEvent) -> DriverResult<()> {
        self.last_telemetry.keys_mask = ev.keys_mask;
        self.last_telemetry.ring_index = ev.ring_index;
        self.last_telemetry.ring_delta = ev.ring_delta;

        if self.config.disable_express_keys {
            self.release_active_express_keys()?;
            return Ok(());
        }

        let changed = self.prev_keys_mask ^ ev.keys_mask;
        if changed != 0 {
            for bit in 0..40usize {
                let mask = 1u64 << bit;
                if (changed & mask) != 0 {
                    let pressed = (ev.keys_mask & mask) != 0;
                    if let Some(action) = self.config.express_key_action(bit).cloned() {
                        debug!(key_index = bit + 1, pressed, "express key alterada");
                        self.apply_button_action(&action, pressed)?;
                        if pressed
                            && self.config.osd_notifications
                            && !matches!(
                                action,
                                Action::None | Action::OpenGui | Action::TogglePrecisionMode
                            )
                        {
                            send_osd_notification(
                                &format!("Tecla K{}", bit + 1),
                                &self.config.express_key_label(bit),
                            );
                        }
                    }
                }
            }
            self.prev_keys_mask = ev.keys_mask;
        }

        if ev.ring_delta != 0 {
            let mode_idx = if ev.ring_index == 0 {
                self.ring_1_mode_idx
            } else {
                0
            };
            let ring = self.config.ring_mode_at(ev.ring_index, mode_idx);
            let action = if ev.ring_delta > 0 {
                ring.cw.clone()
            } else {
                ring.ccw.clone()
            };
            self.trigger_pulse_action(&action)?;
        }

        Ok(())
    }

    fn apply_button_action(&mut self, action: &Action, pressed: bool) -> DriverResult<()> {
        let value = i32::from(pressed);
        match action {
            Action::None => Ok(()),
            Action::CycleRingMode => {
                if pressed {
                    let count = self.config.ring_mode_count(0);
                    self.ring_1_mode_idx = (self.ring_1_mode_idx + 1) % count;
                    debug!(
                        ring_mode = self.ring_1_mode_idx + 1,
                        "modo do anel alternado"
                    );
                }
                Ok(())
            }
            Action::ToggleEraser => {
                if pressed {
                    self.eraser_toggled = !self.eraser_toggled;
                    debug!(eraser = self.eraser_toggled, "modo borracha alternado");
                }
                Ok(())
            }
            Action::TogglePrecisionMode => {
                if pressed {
                    self.precision_mode = !self.precision_mode;
                    self.config.precision_mode = self.precision_mode;
                    self.last_telemetry.precision_mode = self.precision_mode;
                    debug!(
                        precision = self.precision_mode,
                        "modo de precisão alternado"
                    );
                    if self.config.osd_notifications {
                        let state_txt = if self.precision_mode {
                            "Ativado"
                        } else {
                            "Desativado"
                        };
                        send_osd_notification(
                            "Modo de Precisão",
                            &format!("Modo de precisão: {state_txt}"),
                        );
                    }
                }
                Ok(())
            }
            Action::ToggleBrushEraserKeys => {
                if pressed {
                    let key = if self.brush_eraser_use_e {
                        Key::KEY_E
                    } else {
                        Key::KEY_B
                    };
                    self.brush_eraser_use_e = !self.brush_eraser_use_e;
                    self.active_brush_eraser_key = Some(key);
                    self.sink
                        .emit_pad(&[InputEvent::new(EventType::KEY, key.code(), 1)])?;
                } else if let Some(key) = self.active_brush_eraser_key.take() {
                    self.sink
                        .emit_pad(&[InputEvent::new(EventType::KEY, key.code(), 0)])?;
                }
                Ok(())
            }
            Action::MouseLeft => {
                self.sink
                    .emit_pad(&[InputEvent::new(EventType::KEY, Key::BTN_LEFT.code(), value)])
            }
            Action::MouseRight => self.sink.emit_pad(&[InputEvent::new(
                EventType::KEY,
                Key::BTN_RIGHT.code(),
                value,
            )]),
            Action::MouseMiddle => self.sink.emit_pad(&[InputEvent::new(
                EventType::KEY,
                Key::BTN_MIDDLE.code(),
                value,
            )]),
            Action::MouseDoubleLeft => {
                if pressed {
                    // Cada transição precisa de seu próprio SYN_REPORT para o libinput/kernel.
                    for state in [1, 0, 1, 0] {
                        self.sink.emit_pad(&[InputEvent::new(
                            EventType::KEY,
                            Key::BTN_LEFT.code(),
                            state,
                        )])?;
                    }
                }
                Ok(())
            }
            Action::MouseWheel { delta } => {
                if pressed {
                    self.sink.emit_pad(&[InputEvent::new(
                        EventType::RELATIVE,
                        RelativeAxisType::REL_WHEEL.0,
                        *delta,
                    )])?;
                }
                Ok(())
            }
            Action::KeyCombo { keys } => {
                if keys.is_empty() {
                    return Ok(());
                }
                let unique: BTreeSet<u16> = keys.iter().copied().collect();
                let mut events = Vec::with_capacity(unique.len());
                if pressed {
                    for &code in keys {
                        events.push(InputEvent::new(EventType::KEY, code, 1));
                    }
                } else {
                    for &code in keys.iter().rev() {
                        events.push(InputEvent::new(EventType::KEY, code, 0));
                    }
                }
                self.sink.emit_pad(&events)
            }
            Action::TabletButton { index } => {
                let key_code = Key::BTN_0.code() + u16::from((*index).min(7));
                self.sink.emit_native_pad(&[
                    InputEvent::new(EventType::KEY, key_code, value),
                    InputEvent::new(
                        EventType::ABSOLUTE,
                        AbsoluteAxisType::ABS_MISC.0,
                        if pressed { 15 } else { 0 },
                    ),
                ])
            }
            Action::RunCommand { command } => {
                if pressed && !command.trim().is_empty() {
                    spawn_shell_command(command);
                }
                Ok(())
            }
            Action::OpenGui => {
                if pressed {
                    launch_gui_process();
                }
                Ok(())
            }
        }
    }

    /// Dispara um pressionamento/liberação virtual da Express Key informada (0-based, ex.: `0` = K1).
    pub fn trigger_express_key(&mut self, index: usize) -> DriverResult<()> {
        if let Some(action) = self.config.express_key_action(index).cloned() {
            self.apply_button_action(&action, true)?;
            if self.config.osd_notifications
                && !matches!(
                    action,
                    Action::None | Action::OpenGui | Action::TogglePrecisionMode
                )
            {
                send_osd_notification(
                    &format!("Tecla K{}", index + 1),
                    &self.config.express_key_label(index),
                );
            }
            self.apply_button_action(&action, false)?;
        }
        Ok(())
    }

    fn trigger_pulse_action(&mut self, action: &Action) -> DriverResult<()> {
        match action {
            Action::MouseWheel { delta } => self.sink.emit_pad(&[InputEvent::new(
                EventType::RELATIVE,
                RelativeAxisType::REL_WHEEL.0,
                *delta,
            )]),
            _ => {
                self.apply_button_action(action, true)?;
                self.apply_button_action(action, false)
            }
        }
    }
}

/// Localiza o executável ou script Python da GUI instalada/local.
pub fn find_gui_executable() -> Option<PathBuf> {
    let mut candidates = vec![PathBuf::from("/usr/local/bin/ugee-gui")];
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            candidates.push(PathBuf::from(home.trim()).join(".local/bin/ugee-gui"));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("ugee-gui"));
            candidates.push(dir.join("gui/ugee_gui.py"));
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("gui/ugee_gui.py"));
                if let Some(grandparent) = parent.parent() {
                    candidates.push(grandparent.join("gui/ugee_gui.py"));
                }
            }
        }
    }
    candidates.push(PathBuf::from("gui/ugee_gui.py"));
    candidates.into_iter().find(|p| p.exists())
}

fn spawn_shell_command(command: &str) {
    if cfg!(test) {
        return;
    }
    if let Err(err) = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        warn!(command, error = %err, "falha ao executar comando da Express Key");
    }
}

fn launch_gui_process() {
    if cfg!(test) {
        return;
    }
    let Some(gui_path) = find_gui_executable() else {
        warn!("interface gráfica ugee-gui / gui/ugee_gui.py não encontrada");
        return;
    };
    let mut cmd = if gui_path.extension().and_then(|e| e.to_str()) == Some("py") {
        let mut c = Command::new("python3");
        c.arg(&gui_path);
        c
    } else {
        Command::new(&gui_path)
    };
    if let Err(err) = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        warn!(path = %gui_path.display(), error = %err, "falha ao iniciar GUI");
    }
}

fn send_osd_notification(title: &str, body: &str) {
    if cfg!(test) {
        return;
    }
    let _ = Command::new("notify-send")
        .args([
            "-a",
            "UGEE Tablet",
            "-i",
            "input-tablet",
            "-t",
            "1200",
            "-h",
            "string:x-canonical-private-synchronous:ugee-tablet",
            title,
            body,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::transform::ActiveArea;

    #[derive(Default)]
    struct MockSink {
        stylus_batches: Vec<Vec<InputEvent>>,
        pad_batches: Vec<Vec<InputEvent>>,
        native_pad_batches: Vec<Vec<InputEvent>>,
    }

    impl EventSink for MockSink {
        fn emit_stylus(&mut self, events: &[InputEvent]) -> DriverResult<()> {
            self.stylus_batches.push(events.to_vec());
            Ok(())
        }

        fn emit_pad(&mut self, events: &[InputEvent]) -> DriverResult<()> {
            self.pad_batches.push(events.to_vec());
            Ok(())
        }

        fn emit_native_pad(&mut self, events: &[InputEvent]) -> DriverResult<()> {
            self.native_pad_batches.push(events.to_vec());
            Ok(())
        }
    }

    fn sample_pen(x: u32, y: u32, tip: bool) -> PenEvent {
        PenEvent {
            in_range: true,
            tip_switch: tip,
            barrel_1: false,
            barrel_2: false,
            eraser: false,
            relative_hint: false,
            x,
            y,
            pressure: if tip { 2000 } else { 0 },
            tilt_x: 0,
            tilt_y: 0,
            raw_status: if tip { 0xA1 } else { 0xA0 },
        }
    }

    fn has_event(batch: &[InputEvent], ty: EventType, code: u16, value: i32) -> bool {
        batch
            .iter()
            .any(|ev| ev.event_type() == ty && ev.code() == code && ev.value() == value)
    }

    #[test]
    fn proximity_and_pen_eraser_switch_events() -> Result<(), Box<dyn std::error::Error>> {
        let mut dispatcher = TabletDispatcherCore::with_sink(
            MockSink::default(),
            50800,
            30480,
            8191,
            5080,
            DriverConfig::default(),
        );

        // Entrada em proximidade -> emite BTN_TOOL_PEN = 1
        dispatcher.handle_packet(TabletPacket::Pen(sample_pen(1000, 1000, false)))?;
        assert!(has_event(
            &dispatcher.sink().stylus_batches[0],
            EventType::KEY,
            Key::BTN_TOOL_PEN.code(),
            1
        ));

        // Pressionar barrel_2 (ToggleEraser) alterna para BTN_TOOL_RUBBER = 1 e libera BTN_TOOL_PEN = 0
        let mut toggle_pen = sample_pen(1000, 1000, true);
        toggle_pen.barrel_2 = true;
        dispatcher.handle_packet(TabletPacket::Pen(toggle_pen))?;
        let batch1 = &dispatcher.sink().stylus_batches[1];
        assert!(has_event(
            batch1,
            EventType::KEY,
            Key::BTN_TOOL_PEN.code(),
            0
        ));
        assert!(has_event(
            batch1,
            EventType::KEY,
            Key::BTN_TOOL_RUBBER.code(),
            1
        ));

        // Saída de proximidade libera BTN_TOUCH = 0 e BTN_TOOL_RUBBER = 0
        dispatcher.handle_packet(TabletPacket::OutOfRange { raw_status: 0x80 })?;
        let batch2 = &dispatcher.sink().stylus_batches[2];
        assert!(has_event(batch2, EventType::KEY, Key::BTN_TOUCH.code(), 0));
        assert!(has_event(
            batch2,
            EventType::KEY,
            Key::BTN_TOOL_RUBBER.code(),
            0
        ));
        Ok(())
    }

    #[test]
    fn relative_mode_releases_left_click_on_out_of_range_and_mode_switch(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let cfg = DriverConfig {
            relative_mode: false,
            ..DriverConfig::default()
        };
        let mut dispatcher =
            TabletDispatcherCore::with_sink(MockSink::default(), 50800, 30480, 8191, 5080, cfg);

        // Pacote relativo (0xC0) com ponta pressionada -> emite BTN_LEFT = 1 no pad (sem BTN_TOOL_PEN)
        let mut rel_pen = sample_pen(1000, 1000, true);
        rel_pen.relative_hint = true;
        rel_pen.raw_status = 0xC0;
        dispatcher.handle_packet(TabletPacket::Pen(rel_pen))?;
        assert!(dispatcher.sink().stylus_batches.is_empty());
        assert!(has_event(
            &dispatcher.sink().pad_batches[0],
            EventType::KEY,
            Key::BTN_LEFT.code(),
            1
        ));

        // OutOfRange solta BTN_LEFT = 0 no pad
        dispatcher.handle_packet(TabletPacket::OutOfRange { raw_status: 0x80 })?;
        assert!(has_event(
            &dispatcher.sink().pad_batches[1],
            EventType::KEY,
            Key::BTN_LEFT.code(),
            0
        ));

        // Nova sequência: 0xC0 com tip -> volta para 0xA0 absoluto -> solta BTN_LEFT e entra em proximidade Stylus
        dispatcher.handle_packet(TabletPacket::Pen(rel_pen))?;
        dispatcher.handle_packet(TabletPacket::Pen(sample_pen(2000, 2000, false)))?;
        let last_pad = dispatcher
            .sink()
            .pad_batches
            .last()
            .ok_or("esperava emissão no pad")?;
        assert!(has_event(last_pad, EventType::KEY, Key::BTN_LEFT.code(), 0));
        let last_stylus = dispatcher
            .sink()
            .stylus_batches
            .last()
            .ok_or("esperava emissão no stylus")?;
        assert!(has_event(
            last_stylus,
            EventType::KEY,
            Key::BTN_TOOL_PEN.code(),
            1
        ));
        Ok(())
    }

    #[test]
    fn leaving_active_area_releases_touch_and_proximity() -> Result<(), Box<dyn std::error::Error>>
    {
        let cfg = DriverConfig {
            active_area: ActiveArea {
                x_hundredths_in: 100,
                y_hundredths_in: 100,
                w_hundredths_in: 400,
                h_hundredths_in: 200,
            },
            ..DriverConfig::default()
        };
        let mut dispatcher =
            TabletDispatcherCore::with_sink(MockSink::default(), 50800, 30480, 8191, 5080, cfg);

        // Dentro da área ativa (x=2.0" = 10160, y=1.5" = 7620) com ponta pressionada
        dispatcher.handle_packet(TabletPacket::Pen(sample_pen(10160, 7620, true)))?;
        assert!(has_event(
            &dispatcher.sink().stylus_batches[0],
            EventType::KEY,
            Key::BTN_TOUCH.code(),
            1
        ));

        // Arrasta para fora da área ativa (x=100) -> deve soltar BTN_TOUCH = 0 e proximidade
        dispatcher.handle_packet(TabletPacket::Pen(sample_pen(100, 7620, true)))?;
        let batch = &dispatcher.sink().stylus_batches[1];
        assert!(has_event(batch, EventType::KEY, Key::BTN_TOUCH.code(), 0));
        assert!(has_event(
            batch,
            EventType::KEY,
            Key::BTN_TOOL_PEN.code(),
            0
        ));
        Ok(())
    }

    #[test]
    fn double_left_and_toggle_brush_eraser_emit_expected_frames(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cfg = DriverConfig::default();
        cfg.express_keys
            .insert("k1".to_string(), Action::from_actid(209, None)); // MouseDoubleLeft
        cfg.express_keys
            .insert("k2".to_string(), Action::from_actid(408, None)); // [B]/[E]

        let mut dispatcher =
            TabletDispatcherCore::with_sink(MockSink::default(), 50800, 30480, 8191, 5080, cfg);

        // K1 -> 4 batches separados no pad (1, 0, 1, 0)
        dispatcher.handle_packet(TabletPacket::ExpressKey(ExpressKeyEvent {
            keys_mask: 1 << 0,
            ring_index: 0,
            ring_delta: 0,
        }))?;
        assert_eq!(dispatcher.sink().pad_batches.len(), 4);
        assert_eq!(
            dispatcher
                .sink()
                .pad_batches
                .iter()
                .map(|b| b[0].value())
                .collect::<Vec<_>>(),
            vec![1, 0, 1, 0]
        );

        // Solta K1, pressiona K2 -> emite KEY_B = 1 e depois KEY_B = 0
        dispatcher.handle_packet(TabletPacket::ExpressKey(ExpressKeyEvent {
            keys_mask: 1 << 1,
            ring_index: 0,
            ring_delta: 0,
        }))?;
        assert!(has_event(
            &dispatcher.sink().pad_batches[4],
            EventType::KEY,
            Key::KEY_B.code(),
            1
        ));
        dispatcher.handle_packet(TabletPacket::ExpressKey(ExpressKeyEvent {
            keys_mask: 0,
            ring_index: 0,
            ring_delta: 0,
        }))?;
        assert!(has_event(
            &dispatcher.sink().pad_batches[5],
            EventType::KEY,
            Key::KEY_B.code(),
            0
        ));

        // Novo clique K2 -> alterna para KEY_E
        dispatcher.handle_packet(TabletPacket::ExpressKey(ExpressKeyEvent {
            keys_mask: 1 << 1,
            ring_index: 0,
            ring_delta: 0,
        }))?;
        assert!(has_event(
            &dispatcher.sink().pad_batches[6],
            EventType::KEY,
            Key::KEY_E.code(),
            1
        ));
        Ok(())
    }

    #[test]
    fn releases_held_keys_on_config_reload_and_disable_express_keys(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cfg = DriverConfig::default();
        cfg.express_keys.insert(
            "k1".to_string(),
            Action::combo(&[Key::KEY_LEFTCTRL, Key::KEY_Z]),
        );
        cfg.pen_buttons.barrel_1 = Action::MouseRight;

        let mut dispatcher =
            TabletDispatcherCore::with_sink(MockSink::default(), 50800, 30480, 8191, 5080, cfg);

        // Segura K1 (Ctrl+Z) e Barrel 1 (BTN_RIGHT)
        dispatcher.handle_packet(TabletPacket::ExpressKey(ExpressKeyEvent {
            keys_mask: 1 << 0,
            ring_index: 0,
            ring_delta: 0,
        }))?;
        let mut held_pen = sample_pen(2000, 2000, false);
        held_pen.barrel_1 = true;
        dispatcher.handle_packet(TabletPacket::Pen(held_pen))?;

        // Hot-reload trocando o mapeamento -> deve liberar antes BTN_RIGHT e Ctrl+Z
        let mut next_cfg = DriverConfig::default();
        next_cfg
            .express_keys
            .insert("k1".to_string(), Action::combo(&[Key::KEY_SPACE]));
        dispatcher.update_config(next_cfg.clone());

        let pad_after_reload = &dispatcher.sink().pad_batches;
        assert!(pad_after_reload.iter().any(|b| has_event(
            b,
            EventType::KEY,
            Key::BTN_RIGHT.code(),
            0
        )));
        assert!(pad_after_reload.iter().any(|b| has_event(
            b,
            EventType::KEY,
            Key::KEY_LEFTCTRL.code(),
            0
        )));
        assert!(pad_after_reload.iter().any(|b| has_event(
            b,
            EventType::KEY,
            Key::KEY_Z.code(),
            0
        )));

        // Segura K1 (KEY_SPACE) e depois ativa disable_express_keys durante atualização/pacote seguinte
        dispatcher.handle_packet(TabletPacket::ExpressKey(ExpressKeyEvent {
            keys_mask: 1 << 0,
            ring_index: 0,
            ring_delta: 0,
        }))?;
        let batches_before_disable = dispatcher.sink().pad_batches.len();

        let mut disabled_cfg = next_cfg;
        disabled_cfg.disable_express_keys = true;
        dispatcher.update_config(disabled_cfg);
        assert!(dispatcher.sink().pad_batches[batches_before_disable..]
            .iter()
            .any(|b| has_event(b, EventType::KEY, Key::KEY_SPACE.code(), 0)));

        // Mesmo que um pacote chegue ainda com K1 pressionado após desativação, não trava teclas
        dispatcher.handle_packet(TabletPacket::ExpressKey(ExpressKeyEvent {
            keys_mask: 1 << 0,
            ring_index: 0,
            ring_delta: 0,
        }))?;
        Ok(())
    }

    #[test]
    fn precision_mode_and_trigger_key_tablet_button_behave_accurately(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cfg = DriverConfig {
            precision_mode: true,
            ..DriverConfig::default()
        };
        cfg.express_keys
            .insert("k1".to_string(), Action::TabletButton { index: 2 });

        let mut dispatcher =
            TabletDispatcherCore::with_sink(MockSink::default(), 50800, 30480, 8191, 5080, cfg);

        // Coordenada (0, 0) com precision_mode -> mapeia para 25% do espaço (12700, 7620)
        dispatcher.handle_packet(TabletPacket::Pen(sample_pen(0, 0, false)))?;
        assert_eq!(dispatcher.telemetry().mapped_x, 12700);
        assert_eq!(dispatcher.telemetry().mapped_y, 7620);
        assert!(dispatcher.telemetry().precision_mode);
        assert!(dispatcher.telemetry().inside_area);

        // trigger_express_key(0) dispara BTN_2 press (1) e release (0) no dispositivo Pad nativo
        dispatcher.trigger_express_key(0)?;
        let batches = &dispatcher.sink().native_pad_batches;
        assert_eq!(batches.len(), 2);
        assert!(has_event(&batches[0], EventType::KEY, Key::BTN_2.code(), 1));
        assert!(has_event(&batches[1], EventType::KEY, Key::BTN_2.code(), 0));
        Ok(())
    }
}
