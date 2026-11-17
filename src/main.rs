use anyhow::{Context, Result};
use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, RwLock};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;
use ugee_driver::config::DriverConfig;
use ugee_driver::error::DriverError;
use ugee_driver::hid::{
    build_led_brightness_report, pid_quirks, DiscoveredTablet, SystemEvent, TabletDevice,
    TabletPacket,
};
use ugee_driver::ipc::{
    default_socket_path, drain_offline_control_messages, send_ipc_request, spawn_ipc_thread,
    DaemonControlMsg, DaemonSnapshot, IpcRequest, IpcResponse, IpcServer, LiveTelemetry,
};
use ugee_driver::uinput::{find_gui_executable, TabletDispatcher};

static RUNNING: AtomicBool = AtomicBool::new(true);

extern "C" fn handle_signal(_sig: libc::c_int) {
    RUNNING.store(false, Ordering::SeqCst);
}

#[derive(Debug)]
enum CommandMode {
    Run,
    Gui,
    Status,
    Reload,
    Probe,
    CaptureMs(u64),
    PrintDefaultConfig,
    Help,
}

#[derive(Debug)]
struct CliArgs {
    mode: CommandMode,
    config_path: Option<PathBuf>,
}

fn parse_args() -> Result<CliArgs> {
    let mut mode = CommandMode::Run;
    let mut config_path = None;
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--gui" => mode = CommandMode::Gui,
            "--status" => mode = CommandMode::Status,
            "--reload" => mode = CommandMode::Reload,
            "--probe" => mode = CommandMode::Probe,
            "--print-default-config" => mode = CommandMode::PrintDefaultConfig,
            "--help" | "-h" => mode = CommandMode::Help,
            "--capture-ms" => {
                let raw = args
                    .next()
                    .context("esperado valor numérico em ms após --capture-ms")?;
                let ms: u64 = raw
                    .parse()
                    .with_context(|| format!("valor inválido para --capture-ms: {raw}"))?;
                mode = CommandMode::CaptureMs(ms);
            }
            "--config" | "-c" => {
                let raw = args
                    .next()
                    .context("esperado caminho de arquivo TOML após --config")?;
                config_path = Some(PathBuf::from(raw));
            }
            other => {
                anyhow::bail!("argumento desconhecido: {other} (use --help para ajuda)");
            }
        }
    }

    Ok(CliArgs { mode, config_path })
}

fn print_help() {
    println!(
        "ugee-driver - Driver Linux user-space para mesas digitalizadoras UGEE\n\
         \n\
         USO:\n\
         \tugee-driver [OPÇÕES]\n\
         \n\
         OPÇÕES:\n\
         \t--config, -c <arquivo.toml>   Carrega perfil de configuração TOML\n\
         \t--gui                         Abre a interface gráfica GTK4/Libadwaita (Python) de configuração\n\
         \t--status                      Consulta status e telemetria do daemon via socket IPC\n\
         \t--reload                      Solicita recarregamento do arquivo TOML em execução\n\
         \t--print-default-config        Imprime configuração TOML padrão e encerra\n\
         \t--probe                       Inspeciona mesa conectada, envia handshake e valida uinput\n\
         \t--capture-ms <ms>             Executa o loop do driver por <ms> milissegundos (teste)\n\
         \t--help, -h                    Exibe esta ajuda"
    );
}

fn run_gui(config_path: Option<PathBuf>) -> Result<()> {
    let gui_path = find_gui_executable().context(
        "interface gráfica Python (ugee-gui / gui/ugee_gui.py) não encontrada no sistema",
    )?;
    let mut cmd = if gui_path.extension().and_then(|e| e.to_str()) == Some("py") {
        let mut c = std::process::Command::new("python3");
        c.arg(&gui_path);
        c
    } else {
        std::process::Command::new(&gui_path)
    };
    if let Some(cfg) = config_path {
        cmd.arg("--config").arg(cfg);
    }
    let status = cmd
        .status()
        .with_context(|| format!("falha ao iniciar GUI em {}", gui_path.display()))?;
    if !status.success() {
        anyhow::bail!("GUI encerrada com status {}", status);
    }
    Ok(())
}

fn load_config(explicit_path: Option<&PathBuf>) -> Result<(DriverConfig, PathBuf)> {
    if let Some(p) = explicit_path {
        let cfg = DriverConfig::load_from_path(p)
            .with_context(|| format!("falha ao carregar configuração em {}", p.display()))?;
        return Ok((cfg, p.clone()));
    }

    let default_path = DriverConfig::default_config_path();
    if default_path.exists() {
        let cfg = DriverConfig::load_from_path(&default_path).with_context(|| {
            format!(
                "falha ao carregar configuração em {}",
                default_path.display()
            )
        })?;
        return Ok((cfg, default_path));
    }

    Ok((DriverConfig::default(), default_path))
}

fn install_signal_handlers() {
    // SAFETY: `handle_signal` é async-signal-safe (apenas escreve em AtomicBool).
    unsafe {
        libc::signal(
            libc::SIGINT,
            handle_signal as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGTERM,
            handle_signal as *const () as libc::sighandler_t,
        );
    }
}

fn run_probe(config: &DriverConfig) -> Result<()> {
    let candidates = TabletDevice::discover_all()?;
    if candidates.is_empty() {
        anyhow::bail!("nenhuma interface hidraw com VID 0x28BD encontrada");
    }

    println!("Interfaces HID UGEE detectadas ({}):", candidates.len());
    for cand in &candidates {
        println!(
            "  - {} | PID={:#06x} | UsagePage={:#06x} Usage={:#06x} | ReportID={:?} In={}B Out={}B | VendorRaw={}",
            cand.hidraw_path.display(),
            cand.pid,
            cand.report_info.usage_page,
            cand.report_info.usage,
            cand.report_info.report_id,
            cand.report_info.input_report_bytes,
            cand.report_info.output_report_bytes,
            cand.report_info.is_ugee_vendor_interface()
        );
    }

    let mut device = TabletDevice::open_first()?;
    let handshake_bytes = device.send_handshake()?;
    let info = device.info();
    let (max_x, max_y, max_p, lpi) = info.physical_bounds();
    let dispatcher = TabletDispatcher::new(info, config.clone())?;
    let (ux, uy, up) = dispatcher.stylus_bounds();

    println!("\nMesa ativa selecionada: {}", info.hidraw_path.display());
    println!(
        "  USB Device: {}",
        info.usb_dev_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "indisponível".to_string())
    );
    println!(
        "  Manufacturer: {:?} | Product: {:?} | Serial: {:?}",
        info.descriptors.manufacturer,
        info.descriptors.product_name,
        info.descriptors.serial_number
    );
    println!(
        "  ICVersion: {:?} | ICData: {:?} | Perfil: {}",
        info.descriptors.ic_version, info.descriptors.ic_data, info.profile.name
    );
    println!(
        "  Dimensões físicas (0x64): {}x{} | MaxPressure={} | LPI={} | ExpressKeys={}",
        max_x,
        max_y,
        max_p,
        lpi,
        info.express_key_count()
    );
    println!("  Handshake enviado: {:02X?}", handshake_bytes);
    println!(
        "  Dispositivos uinput criados com sucesso (Stylus={}x{}, P={})",
        ux, uy, up
    );

    Ok(())
}

fn sync_snapshot_connected(
    snapshot: Option<&Arc<RwLock<DaemonSnapshot>>>,
    info: &DiscoveredTablet,
    config: &DriverConfig,
    config_path: &Path,
    telemetry: &LiveTelemetry,
) {
    let Some(shared) = snapshot else {
        return;
    };
    let Ok(mut guard) = shared.write() else {
        return;
    };

    let (max_x, max_y, max_pressure, lpi) = info.physical_bounds();
    guard.connected = true;
    guard.hidraw_path = info.hidraw_path.display().to_string();
    guard.vid = info.vid;
    guard.pid = info.pid;
    guard.manufacturer = info.descriptors.manufacturer.clone().unwrap_or_default();
    guard.product_name = info
        .descriptors
        .product_name
        .clone()
        .unwrap_or_else(|| info.hid_name.clone());
    guard.serial_number = info.descriptors.serial_number.clone().unwrap_or_default();
    guard.ic_version = info.descriptors.ic_version.clone().unwrap_or_default();
    guard.profile_name = info.profile.name.to_string();
    guard.max_x = max_x;
    guard.max_y = max_y;
    guard.max_pressure = max_pressure;
    guard.lpi = lpi;
    guard.express_keys_count = info.express_key_count();
    guard.battery_level = None;
    guard.led_brightness = None;
    guard.telemetry = telemetry.clone();
    guard.config = config.clone();
    guard.config_path = config_path.to_path_buf();
}

fn sync_snapshot_disconnected(snapshot: Option<&Arc<RwLock<DaemonSnapshot>>>) {
    let Some(shared) = snapshot else {
        return;
    };
    if let Ok(mut guard) = shared.write() {
        guard.connected = false;
        guard.hidraw_path.clear();
        guard.telemetry = LiveTelemetry::default();
    }
}

fn apply_runtime_config(
    device: &TabletDevice,
    dispatcher: &mut TabletDispatcher,
    snapshot: Option<&Arc<RwLock<DaemonSnapshot>>>,
    config_path: &Path,
    next_config: DriverConfig,
    save_to_disk: bool,
) -> IpcResponse {
    if save_to_disk {
        if let Err(err) = next_config.save_to_path(config_path) {
            return IpcResponse::Error {
                message: format!("falha ao salvar configuração: {err}"),
            };
        }
    }

    if dispatcher.needs_device_recreate(&next_config) {
        match TabletDispatcher::new(device.info(), next_config.clone()) {
            Ok(new_dispatcher) => {
                *dispatcher = new_dispatcher;
            }
            Err(err) => {
                return IpcResponse::Error {
                    message: format!("falha ao recriar dispositivo uinput: {err}"),
                };
            }
        }
    } else {
        dispatcher.update_config(next_config.clone());
    }

    if let Some(shared) = snapshot {
        if let Ok(mut guard) = shared.write() {
            guard.config = next_config;
            guard.config_path = config_path.to_path_buf();
            guard.telemetry = dispatcher.telemetry().clone();
        }
    }

    IpcResponse::Ok {
        message: "configuração aplicada ao driver em execução".to_string(),
    }
}

fn drain_control_messages(
    device: &mut TabletDevice,
    dispatcher: &mut TabletDispatcher,
    snapshot: Option<&Arc<RwLock<DaemonSnapshot>>>,
    config_path: &Path,
    control_rx: Option<&mpsc::Receiver<DaemonControlMsg>>,
) {
    let Some(rx) = control_rx else {
        return;
    };

    while let Ok(msg) = rx.try_recv() {
        if msg.is_canceled() {
            continue;
        }
        match msg {
            DaemonControlMsg::UpdateConfig {
                config,
                save_to_disk,
                reply,
                ..
            } => {
                let resp = apply_runtime_config(
                    device,
                    dispatcher,
                    snapshot,
                    config_path,
                    *config,
                    save_to_disk,
                );
                let _ = reply.send(resp);
            }
            DaemonControlMsg::ReloadConfig { reply, .. } => {
                match DriverConfig::load_from_path(config_path) {
                    Ok(cfg) => {
                        let resp = apply_runtime_config(
                            device,
                            dispatcher,
                            snapshot,
                            config_path,
                            cfg,
                            false,
                        );
                        let _ = reply.send(resp);
                    }
                    Err(err) => {
                        let _ = reply.send(IpcResponse::Error {
                            message: format!(
                                "falha ao recarregar {}: {err}",
                                config_path.display()
                            ),
                        });
                    }
                }
            }
            DaemonControlMsg::SetLedBrightness {
                val1, val2, reply, ..
            } => {
                let report_len = device.info().output_report_len();
                let cmd = build_led_brightness_report(val1, val2, report_len);
                let resp = match device.send_output_report(&cmd) {
                    Ok(()) => {
                        if let Some(shared) = snapshot {
                            if let Ok(mut guard) = shared.write() {
                                guard.led_brightness = Some(val2.unwrap_or(val1));
                            }
                        }
                        IpcResponse::Ok {
                            message: format!("brilho LED enviado (val1={val1}, val2={val2:?})"),
                        }
                    }
                    Err(err) => IpcResponse::Error {
                        message: format!("falha ao enviar brilho LED: {err}"),
                    },
                };
                let _ = reply.send(resp);
            }
            DaemonControlMsg::TriggerKey { index, reply, .. } => {
                let resp = match dispatcher.trigger_express_key(index) {
                    Ok(()) => {
                        if let Some(shared) = snapshot {
                            if let Ok(mut guard) = shared.write() {
                                guard.config.precision_mode = dispatcher.config().precision_mode;
                                guard.telemetry = dispatcher.telemetry().clone();
                            }
                        }
                        IpcResponse::Ok {
                            message: format!("tecla K{} acionada via IPC", index + 1),
                        }
                    }
                    Err(err) => IpcResponse::Error {
                        message: format!("falha ao acionar tecla K{}: {err}", index + 1),
                    },
                };
                let _ = reply.send(resp);
            }
        }
    }
}

fn run_session(
    config: &DriverConfig,
    config_path: &Path,
    max_duration: Option<Duration>,
    snapshot: Option<&Arc<RwLock<DaemonSnapshot>>>,
    control_rx: Option<&mpsc::Receiver<DaemonControlMsg>>,
) -> Result<()> {
    let mut device = TabletDevice::open_first()?;
    device.send_handshake()?;
    let mut dispatcher = TabletDispatcher::new(device.info(), config.clone())?;
    sync_snapshot_connected(
        snapshot,
        device.info(),
        config,
        config_path,
        dispatcher.telemetry(),
    );

    let started = Instant::now();
    let mut packets_processed: u64 = 0;

    let result = (|| -> Result<()> {
        while RUNNING.load(Ordering::SeqCst) {
            if let Some(limit) = max_duration {
                if started.elapsed() >= limit {
                    break;
                }
            }

            drain_control_messages(
                &mut device,
                &mut dispatcher,
                snapshot,
                config_path,
                control_rx,
            );

            match device.read_packet(50) {
                Ok(Some(packet)) => {
                    packets_processed += 1;
                    match packet {
                        TabletPacket::System(SystemEvent::Wake) => {
                            handle_wake_event(&mut device)?;
                        }
                        TabletPacket::System(SystemEvent::Battery { raw_level }) => {
                            if let Some(shared) = snapshot {
                                if let Ok(mut guard) = shared.write() {
                                    guard.battery_level = Some(raw_level);
                                    guard.packets_processed = packets_processed;
                                }
                            }
                        }
                        TabletPacket::System(SystemEvent::LedBrightness { target, value }) => {
                            let level = if value > 0 { value } else { target };
                            if let Some(shared) = snapshot {
                                if let Ok(mut guard) = shared.write() {
                                    guard.led_brightness = Some(level);
                                    guard.packets_processed = packets_processed;
                                }
                            }
                        }
                        other => {
                            if let Err(err) = dispatcher.handle_packet(other) {
                                warn!(error = %err, "erro ao emitir evento uinput");
                            }
                            if let Some(shared) = snapshot {
                                if let Ok(mut guard) = shared.write() {
                                    guard.packets_processed = packets_processed;
                                    guard.config.precision_mode =
                                        dispatcher.config().precision_mode;
                                    guard.telemetry = dispatcher.telemetry().clone();
                                }
                            }
                        }
                    }
                }
                Ok(None) => {}
                Err(DriverError::DeviceDisconnected) => {
                    return Err(DriverError::DeviceDisconnected.into());
                }
                Err(err) => return Err(err.into()),
            }
        }
        Ok(())
    })();

    sync_snapshot_disconnected(snapshot);
    if let (Some(shared), Some(rx)) = (snapshot, control_rx) {
        drain_offline_control_messages(shared, rx);
    }
    info!(packets_processed, "sessão de captura encerrada");
    result
}

fn handle_wake_event(device: &mut TabletDevice) -> Result<()> {
    debug!("evento de wake (0xF8 0x02 0x01) recebido; reenviando handshake");
    device.send_handshake()?;

    let pid = device.info().pid;
    if pid_quirks(pid).dual_led_query_on_wake {
        let report_len = device.info().output_report_len();
        for arg1 in [1u8, 2u8] {
            let cmd = build_led_brightness_report(arg1, None, report_len);
            device.send_output_report(&cmd)?;
        }
    }
    Ok(())
}

fn run_daemon(initial_config: &DriverConfig, config_path: &Path) -> Result<()> {
    install_signal_handlers();

    let socket_path = default_socket_path();
    let ipc_server = IpcServer::bind(socket_path.clone()).with_context(|| {
        format!(
            "não foi possível iniciar servidor IPC em {}",
            socket_path.display()
        )
    })?;
    info!(
        socket = %socket_path.display(),
        config = %config_path.display(),
        "servidor IPC ativo para controle e GUI"
    );

    let initial_snapshot = DaemonSnapshot {
        config: initial_config.clone(),
        config_path: config_path.to_path_buf(),
        ..DaemonSnapshot::default()
    };
    let snapshot = Arc::new(RwLock::new(initial_snapshot));
    let (control_tx, control_rx) = mpsc::channel();
    let ipc_running = Arc::new(AtomicBool::new(true));
    let ipc_handle = spawn_ipc_thread(
        ipc_server,
        Arc::clone(&snapshot),
        control_tx,
        Arc::clone(&ipc_running),
    );

    let loop_res = (|| -> Result<()> {
        while RUNNING.load(Ordering::SeqCst) {
            drain_offline_control_messages(&snapshot, &control_rx);
            let active_config = snapshot
                .read()
                .map(|g| g.config.clone())
                .unwrap_or_else(|_| initial_config.clone());

            match run_session(
                &active_config,
                config_path,
                None,
                Some(&snapshot),
                Some(&control_rx),
            ) {
                Ok(()) => break,
                Err(err) => {
                    if !RUNNING.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Some(driver_err) = err.downcast_ref::<DriverError>() {
                        if driver_err.is_permission_denied() {
                            return Err(err);
                        }
                        if matches!(driver_err, DriverError::DeviceNotFound) {
                            info!("aguardando conexão de mesa UGEE (VID 0x28BD)...");
                        } else {
                            warn!(error = %err, "conexão encerrada ou erro transitório; tentando reconectar");
                        }
                    } else {
                        warn!(error = %err, "conexão encerrada ou erro transitório; tentando reconectar");
                    }

                    for _ in 0..40 {
                        if !RUNNING.load(Ordering::SeqCst) {
                            break;
                        }
                        drain_offline_control_messages(&snapshot, &control_rx);
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
            }
        }
        Ok(())
    })();

    ipc_running.store(false, Ordering::SeqCst);
    let _ = ipc_handle.join();
    loop_res
}

fn run_status_cmd() -> Result<()> {
    let socket_path = default_socket_path();
    let resp = send_ipc_request(&socket_path, &IpcRequest::GetStatus).with_context(|| {
        format!(
            "falha ao consultar daemon em {} (o ugee-driver está rodando?)",
            socket_path.display()
        )
    })?;

    match resp {
        IpcResponse::Status(snap) => {
            println!("Daemon UGEE IPC ({}):", socket_path.display());
            println!(
                "  Conectado: {}",
                if snap.connected { "sim" } else { "não" }
            );
            println!(
                "  Dispositivo: {} ({:04X}:{:04X}) | Perfil: {} | hidraw: {}",
                if snap.product_name.is_empty() {
                    "N/D"
                } else {
                    &snap.product_name
                },
                snap.vid,
                snap.pid,
                snap.profile_name,
                if snap.hidraw_path.is_empty() {
                    "N/D"
                } else {
                    &snap.hidraw_path
                }
            );
            println!(
                "  Dimensões: {}x{} @ {} LPI | MaxPress={} | ExpressKeys={}",
                snap.max_x, snap.max_y, snap.lpi, snap.max_pressure, snap.express_keys_count
            );
            println!(
                "  Pacotes processados: {} | Bateria: {:?} | LED: {:?}",
                snap.packets_processed, snap.battery_level, snap.led_brightness
            );
            println!(
                "  Config ativa: {} | Orientação: {:?} | Área: {:?}",
                snap.config_path.display(),
                snap.config.orientation,
                snap.config.active_area
            );
            Ok(())
        }
        IpcResponse::Ok { message } => {
            println!("{message}");
            Ok(())
        }
        IpcResponse::Error { message } => anyhow::bail!("erro retornado pelo daemon: {message}"),
    }
}

fn run_reload_cmd() -> Result<()> {
    let socket_path = default_socket_path();
    let resp = send_ipc_request(&socket_path, &IpcRequest::ReloadConfig).with_context(|| {
        format!(
            "falha ao enviar --reload para {} (o ugee-driver está rodando?)",
            socket_path.display()
        )
    })?;

    match resp {
        IpcResponse::Ok { message } => {
            println!("{message}");
            Ok(())
        }
        IpcResponse::Error { message } => anyhow::bail!("erro ao recarregar: {message}"),
        IpcResponse::Status(_) => Ok(()),
    }
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = match parse_args() {
        Ok(a) => a,
        Err(err) => {
            error!(error = %err, "erro nos argumentos CLI");
            return ExitCode::FAILURE;
        }
    };

    if matches!(args.mode, CommandMode::Help) {
        print_help();
        return ExitCode::SUCCESS;
    }

    if matches!(args.mode, CommandMode::Gui) {
        return match run_gui(args.config_path) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                error!(error = %err, "falha ao iniciar interface gráfica GTK4");
                ExitCode::FAILURE
            }
        };
    }

    if matches!(args.mode, CommandMode::Status) {
        return match run_status_cmd() {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                error!(error = %err, "falha no comando --status");
                ExitCode::FAILURE
            }
        };
    }

    if matches!(args.mode, CommandMode::Reload) {
        return match run_reload_cmd() {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                error!(error = %err, "falha no comando --reload");
                ExitCode::FAILURE
            }
        };
    }

    let (config, config_path) = match load_config(args.config_path.as_ref()) {
        Ok(loaded) => loaded,
        Err(err) => {
            error!(error = %err, "erro ao carregar configuração");
            return ExitCode::FAILURE;
        }
    };

    let res = match args.mode {
        CommandMode::PrintDefaultConfig => config
            .to_toml_string()
            .map(|txt| {
                print!("{txt}");
            })
            .map_err(anyhow::Error::from),
        CommandMode::Probe => run_probe(&config),
        CommandMode::CaptureMs(ms) => {
            install_signal_handlers();
            run_session(
                &config,
                &config_path,
                Some(Duration::from_millis(ms)),
                None,
                None,
            )
        }
        CommandMode::Run => run_daemon(&config, &config_path),
        CommandMode::Gui | CommandMode::Status | CommandMode::Reload | CommandMode::Help => Ok(()),
    };

    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            error!(error = %err, "falha fatal no ugee-driver");
            if err
                .downcast_ref::<DriverError>()
                .is_some_and(DriverError::is_permission_denied)
            {
                eprintln!(
                    "\nDica: falta permissão em /dev/hidraw* ou /dev/uinput para usuário sem root.\n\
                     Instale as regras udev incluídas no projeto (prefixo 60- para rodar antes de 73-seat-late.rules):\n\
                     \tsudo rm -f /etc/udev/rules.d/99-ugee.rules\n\
                     \tsudo cp udev/60-ugee.rules /etc/udev/rules.d/60-ugee.rules\n\
                     \tsudo udevadm control --reload-rules && sudo udevadm trigger\n\
                     (Se necessário, reconecte o cabo USB da mesa após aplicar a regra.)"
                );
            }
            ExitCode::FAILURE
        }
    }
}
