use serde::{Deserialize, Serialize};

/// Ângulo de rotação da mesa digitalizadora (`Orientation` 0..3 em `Ugee_Tablet.xml`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    #[default]
    Deg0 = 0,
    Deg90 = 1,
    Deg180 = 2,
    Deg270 = 3,
}

/// Parâmetros de calibração de paralaxe `<SC fKX fXLC fKY fYLC/>`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScreenCalibration {
    pub enabled: bool,
    pub f_kx: f64,
    pub f_xlc: f64,
    pub f_ky: f64,
    pub f_ylc: f64,
}

impl Default for ScreenCalibration {
    fn default() -> Self {
        Self {
            enabled: false,
            f_kx: 1.0,
            f_xlc: 1.0,
            f_ky: 1.0,
            f_ylc: 1.0,
        }
    }
}

/// Sub-área ativa da mesa digitalizadora em centésimos de polegada (`0` em w/h = área total).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ActiveArea {
    pub x_hundredths_in: u32,
    pub y_hundredths_in: u32,
    pub w_hundredths_in: u32,
    pub h_hundredths_in: u32,
}

/// Retângulo lógico do monitor de destino (quando `w == 0 || h == 0`, ocupa toda a área lógica).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ScreenMapping {
    pub sx: u32,
    pub sy: u32,
    pub sw: u32,
    pub sh: u32,
    pub full_w: u32,
    pub full_h: u32,
}

/// Ponto de controle intermediário da curva de pressão (`x` e `y` em `0.0..=1.0`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CurveControlPoint {
    pub x: f64,
    pub y: f64,
}

/// Parâmetros da curva de pressão (`P0, P1X, P1Y, P2` em `0.0..=1.0` e pontos extras opcionais).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PressureCurveParams {
    pub p0: f64,
    pub p1x: f64,
    pub p1y: f64,
    pub p2: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<CurveControlPoint>,
}

impl Default for PressureCurveParams {
    fn default() -> Self {
        Self {
            p0: 0.0,
            p1x: 0.5,
            p1y: 0.5,
            p2: 1.0,
            points: Vec::new(),
        }
    }
}

/// Tabela de consulta pré-computada (LUT) para mapeamento de pressão sem ponto flutuante no loop USB.
#[derive(Debug, Clone)]
pub struct PressureCurveLut {
    lut: Vec<u16>,
    max_pressure: u16,
    disable_pressure: bool,
}

impl PressureCurveLut {
    /// Constrói a LUT alinhada à curva da GUI:
    /// - 1 ponto de controle: Bézier quadrática `(P0, P1X, P1Y, P2)`.
    /// - 2+ pontos de controle: spline cúbica monótona de Hermite (Fritsch-Carlson).
    pub fn build(params: &PressureCurveParams, max_pressure: u16, disable_pressure: bool) -> Self {
        let max_p = max_pressure.max(1);
        let p0 = params.p0.clamp(0.0, 0.95);
        let p2 = params.p2.clamp(0.10, 1.0);
        let out_scale = f64::from(max_p);

        let spline = if params.points.len() >= 2 {
            Some(MonotoneCubicSpline::from_points(&params.points))
        } else {
            None
        };
        let (p1x, p1y) = if let Some(pt) = params.points.first() {
            (pt.x.clamp(0.05, 0.95), pt.y.clamp(0.05, 0.95))
        } else {
            (params.p1x.clamp(0.05, 0.95), params.p1y.clamp(0.05, 0.95))
        };

        let mut lut = Vec::with_capacity(usize::from(max_p) + 1);
        for raw in 0..=max_p {
            if raw == 0 {
                lut.push(0);
                continue;
            }
            if disable_pressure {
                lut.push(max_p);
                continue;
            }
            let t_in = (f64::from(raw) / out_scale).clamp(0.0, 1.0);
            if t_in <= p0 {
                lut.push(0);
                continue;
            }
            let u = ((t_in - p0) / (1.0 - p0).max(1e-5)).clamp(0.0, 1.0);
            let y_unit = if let Some(ref sp) = spline {
                sp.eval(u)
            } else {
                let a = 1.0 - 2.0 * p1x;
                let b = 2.0 * p1x;
                let s = if a.abs() < 1e-6 {
                    u / b.max(1e-5)
                } else {
                    let disc = (b * b + 4.0 * a * u).max(0.0);
                    (-b + disc.sqrt()) / (2.0 * a)
                }
                .clamp(0.0, 1.0);
                (2.0 * (1.0 - s) * s * p1y + s * s).clamp(0.0, 1.0)
            };

            let y_norm = (y_unit * p2).clamp(0.0, 1.0);
            let out = (y_norm * out_scale).round().clamp(0.0, out_scale) as u16;
            lut.push(out);
        }

        Self {
            lut,
            max_pressure: max_p,
            disable_pressure,
        }
    }

    /// Aplica a curva à pressão bruta recebida da caneta.
    pub fn map(&self, raw_pressure: u16) -> u16 {
        if raw_pressure == 0 {
            return 0;
        }
        if self.disable_pressure {
            return self.max_pressure;
        }
        let idx = usize::from(raw_pressure.min(self.max_pressure));
        self.lut.get(idx).copied().unwrap_or(self.max_pressure)
    }
}

struct MonotoneCubicSpline {
    xs: Vec<f64>,
    ys: Vec<f64>,
    hs: Vec<f64>,
    tangents: Vec<f64>,
}

impl MonotoneCubicSpline {
    fn from_points(points: &[CurveControlPoint]) -> Self {
        let mut sorted: Vec<CurveControlPoint> = points
            .iter()
            .map(|p| CurveControlPoint {
                x: p.x.clamp(0.02, 0.98),
                y: p.y.clamp(0.0, 1.0),
            })
            .collect();
        sorted.sort_by(|a, b| a.x.total_cmp(&b.x));

        let mut xs = Vec::with_capacity(sorted.len() + 2);
        let mut ys = Vec::with_capacity(sorted.len() + 2);
        xs.push(0.0);
        ys.push(0.0);
        for pt in sorted {
            let prev_x = *xs.last().unwrap_or(&0.0);
            let x = pt.x.max(prev_x + 0.005).min(0.99);
            xs.push(x);
            ys.push(pt.y);
        }
        xs.push(1.0);
        ys.push(1.0);

        let n = xs.len();
        let mut hs = Vec::with_capacity(n - 1);
        let mut ds = Vec::with_capacity(n - 1);
        for i in 0..(n - 1) {
            let h = (xs[i + 1] - xs[i]).max(1e-6);
            hs.push(h);
            ds.push((ys[i + 1] - ys[i]) / h);
        }

        let mut tangents = vec![0.0; n];
        tangents[0] = ds[0];
        tangents[n - 1] = ds[n - 2];
        for i in 1..(n - 1) {
            if ds[i - 1] * ds[i] <= 0.0 {
                tangents[i] = 0.0;
            } else {
                tangents[i] = 0.5 * (ds[i - 1] + ds[i]);
            }
        }

        for i in 0..(n - 1) {
            if ds[i].abs() < 1e-9 {
                tangents[i] = 0.0;
                tangents[i + 1] = 0.0;
            } else {
                let alpha = tangents[i] / ds[i];
                let beta = tangents[i + 1] / ds[i];
                let r2 = alpha * alpha + beta * beta;
                if r2 > 9.0 {
                    let tau = 3.0 / r2.sqrt();
                    tangents[i] = tau * alpha * ds[i];
                    tangents[i + 1] = tau * beta * ds[i];
                }
            }
        }

        Self {
            xs,
            ys,
            hs,
            tangents,
        }
    }

    fn eval(&self, u: f64) -> f64 {
        let u = u.clamp(0.0, 1.0);
        let last = self.xs.len() - 1;
        if u <= 0.0 {
            return self.ys[0];
        }
        if u >= 1.0 {
            return self.ys[last];
        }
        let mut seg = 0usize;
        for i in 0..last {
            if u <= self.xs[i + 1] {
                seg = i;
                break;
            }
        }
        let h = self.hs[seg];
        let t = ((u - self.xs[seg]) / h).clamp(0.0, 1.0);
        let t2 = t * t;
        let t3 = t2 * t;
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        let y = h00 * self.ys[seg]
            + h10 * h * self.tangents[seg]
            + h01 * self.ys[seg + 1]
            + h11 * h * self.tangents[seg + 1];
        y.clamp(0.0, 1.0)
    }
}

/// Estado do acumulador de movimento relativo (Mouse Mode em `0x40b954..0x40bc99`).
#[derive(Debug, Default, Clone)]
pub struct RelativeAccumulator {
    anchor: Option<(i32, i32)>,
    rem_x: i32,
    rem_y: i32,
}

impl RelativeAccumulator {
    /// Reinicia âncora e restos (ex: ao sair de proximidade).
    pub fn reset(&mut self) {
        self.anchor = None;
        self.rem_x = 0;
        self.rem_y = 0;
    }

    /// Calcula deslocamento relativo inteiro `(dx, dy)` com acumulação exata de resto (`5 * (10 - speed)`).
    pub fn update(&mut self, x: i32, y: i32, speed: u8) -> (i32, i32) {
        let clamped_speed = i32::from(speed.clamp(1, 9));
        let denom = 5 * (10 - clamped_speed);

        let Some((prev_x, prev_y)) = self.anchor else {
            self.anchor = Some((x, y));
            return (0, 0);
        };

        self.anchor = Some((x, y));
        let accum_x = (x - prev_x) + self.rem_x;
        let accum_y = (y - prev_y) + self.rem_y;

        let dx = accum_x / denom;
        let dy = accum_y / denom;

        if dx != 0 {
            self.rem_x = accum_x % denom;
        } else {
            self.rem_x = accum_x;
        }

        if dy != 0 {
            self.rem_y = accum_y % denom;
        } else {
            self.rem_y = accum_y;
        }

        (dx, dy)
    }
}

/// Saída transformada de um pacote da caneta pronta para emissão via `uinput`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformedStylusCoords {
    /// Posição absoluta na faixa lógica `0..=out_max_x`, `0..=out_max_y`.
    Absolute {
        x: i32,
        y: i32,
        tilt_x: i32,
        tilt_y: i32,
    },
    /// Fora da área ativa configurada (mantém último estado sem mover o cursor).
    OutsideActiveArea,
}

/// Aplica rotação (`0..3`), calibração `<SC>`, recorte de área ativa e mapeamento de tela.
#[allow(clippy::too_many_arguments)]
pub fn transform_absolute_coords(
    raw_x: u32,
    raw_y: u32,
    raw_tilt_x: i8,
    raw_tilt_y: i8,
    phys_max_x: u32,
    phys_max_y: u32,
    lpi: u16,
    orientation: Orientation,
    calibration: ScreenCalibration,
    active_area: ActiveArea,
    screen_mapping: ScreenMapping,
    disable_tilt: bool,
) -> TransformedStylusCoords {
    let w = phys_max_x as i64;
    let h = phys_max_y as i64;
    let x = i64::from(raw_x).clamp(0, w);
    let y = i64::from(raw_y).clamp(0, h);
    let tx = i32::from(raw_tilt_x);
    let ty = i32::from(raw_tilt_y);

    // 1) Rotação conforme `0x40b3d1..0x40b5ea`
    let (rot_x, rot_y, eff_w, eff_h, mut tilt_x, mut tilt_y) = match orientation {
        Orientation::Deg0 => (x, y, w, h, tx, ty),
        Orientation::Deg90 => (h - y, x, h, w, -ty, tx),
        Orientation::Deg180 => (w - x, h - y, w, h, -tx, -ty),
        Orientation::Deg270 => (y, w - x, h, w, ty, -tx),
    };

    if disable_tilt {
        tilt_x = 0;
        tilt_y = 0;
    }

    // 2) Área ativa (`TX, TY, TW, TH` em 0.01 inch -> unidades brutas via LPI)
    let lpi_i64 = i64::from(lpi.max(1));
    let (work_x0, work_y0, work_w, work_h) =
        if active_area.w_hundredths_in > 0 && active_area.h_hundredths_in > 0 {
            let x0 = (i64::from(active_area.x_hundredths_in) * lpi_i64) / 100;
            let y0 = (i64::from(active_area.y_hundredths_in) * lpi_i64) / 100;
            let ww = ((i64::from(active_area.w_hundredths_in) * lpi_i64) / 100).max(1);
            let wh = ((i64::from(active_area.h_hundredths_in) * lpi_i64) / 100).max(1);
            (
                x0,
                y0,
                ww.min(eff_w - x0.min(eff_w - 1)),
                wh.min(eff_h - y0.min(eff_h - 1)),
            )
        } else {
            (0, 0, eff_w.max(1), eff_h.max(1))
        };

    if rot_x < work_x0 || rot_x > work_x0 + work_w || rot_y < work_y0 || rot_y > work_y0 + work_h {
        return TransformedStylusCoords::OutsideActiveArea;
    }

    let mut local_x = rot_x - work_x0;
    let mut local_y = rot_y - work_y0;

    // 3) Calibração linear `<SC>` (`0x40b5ea..0x40b703`)
    if calibration.enabled {
        let kx = if calibration.f_kx.abs() > 1e-6 {
            calibration.f_kx
        } else {
            1.0
        };
        let ky = if calibration.f_ky.abs() > 1e-6 {
            calibration.f_ky
        } else {
            1.0
        };
        let half_w = (work_w / 2) as f64;
        let half_h = (work_h / 2) as f64;
        let cal_x = ((local_x as f64 - half_w) / kx) + 0.5 * calibration.f_xlc * (work_w as f64);
        let cal_y = ((local_y as f64 - half_h) / ky) + 0.5 * calibration.f_ylc * (work_h as f64);
        local_x = (cal_x.floor() as i64).clamp(0, work_w);
        local_y = (cal_y.floor() as i64).clamp(0, work_h);
    }

    // 4) Reescalamento proporcional para o espaço lógico final (e sub-região de monitor opcional)
    let (out_x, out_y) = if screen_mapping.sw > 0
        && screen_mapping.sh > 0
        && screen_mapping.full_w > 0
        && screen_mapping.full_h > 0
    {
        let mapped_px_x = f64::from(screen_mapping.sx)
            + (local_x as f64 / work_w as f64) * f64::from(screen_mapping.sw);
        let mapped_px_y = f64::from(screen_mapping.sy)
            + (local_y as f64 / work_h as f64) * f64::from(screen_mapping.sh);
        let scaled_x = (mapped_px_x / f64::from(screen_mapping.full_w)) * (eff_w as f64);
        let scaled_y = (mapped_px_y / f64::from(screen_mapping.full_h)) * (eff_h as f64);
        (
            (scaled_x.round() as i64).clamp(0, eff_w) as i32,
            (scaled_y.round() as i64).clamp(0, eff_h) as i32,
        )
    } else {
        let scaled_x = (local_x * eff_w) / work_w;
        let scaled_y = (local_y * eff_h) / work_h;
        (
            scaled_x.clamp(0, eff_w) as i32,
            scaled_y.clamp(0, eff_h) as i32,
        )
    };

    TransformedStylusCoords::Absolute {
        x: out_x,
        y: out_y,
        tilt_x,
        tilt_y,
    }
}

/// Retorna a largura e altura máximas do eixo absoluto lógico após aplicar a orientação.
pub fn logical_axis_bounds(
    phys_max_x: u32,
    phys_max_y: u32,
    orientation: Orientation,
) -> (u32, u32) {
    match orientation {
        Orientation::Deg0 | Orientation::Deg180 => (phys_max_x, phys_max_y),
        Orientation::Deg90 | Orientation::Deg270 => (phys_max_y, phys_max_x),
    }
}

/// Calcula as coordenadas normalizadas `(norm_x, norm_y)` em `0.0..=1.0` após aplicar a orientação.
pub fn rotated_normalized_coords(
    raw_x: u32,
    raw_y: u32,
    phys_max_x: u32,
    phys_max_y: u32,
    orientation: Orientation,
) -> (f32, f32) {
    let w = phys_max_x.max(1) as f64;
    let h = phys_max_y.max(1) as f64;
    let rx = (f64::from(raw_x) / w).clamp(0.0, 1.0);
    let ry = (f64::from(raw_y) / h).clamp(0.0, 1.0);
    let (nx, ny) = match orientation {
        Orientation::Deg0 => (rx, ry),
        Orientation::Deg90 => (1.0 - ry, rx),
        Orientation::Deg180 => (1.0 - rx, 1.0 - ry),
        Orientation::Deg270 => (ry, 1.0 - rx),
    };
    (nx as f32, ny as f32)
}

/// Aplica o recorte espacial central (`25%..75%`) do Modo de Precisão (`JingXiMode`).
pub fn apply_precision_mode(x: i32, y: i32, out_max_x: u32, out_max_y: u32) -> (i32, i32) {
    let max_x = out_max_x.max(1) as f64;
    let max_y = out_max_y.max(1) as f64;
    let nx = (f64::from(x) / max_x).clamp(0.0, 1.0);
    let ny = (f64::from(y) / max_y).clamp(0.0, 1.0);
    let px = ((0.25 + nx * 0.50) * max_x).round().clamp(0.0, max_x) as i32;
    let py = ((0.25 + ny * 0.50) * max_y).round().clamp(0.0, max_y) as i32;
    (px, py)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_all_four_quadrants() {
        let w = 50000;
        let h = 30000;
        let cal = ScreenCalibration::default();
        let area = ActiveArea::default();
        let screen = ScreenMapping::default();

        // 0°
        assert_eq!(
            transform_absolute_coords(
                10000,
                5000,
                -10,
                20,
                w,
                h,
                5080,
                Orientation::Deg0,
                cal,
                area,
                screen,
                false
            ),
            TransformedStylusCoords::Absolute {
                x: 10000,
                y: 5000,
                tilt_x: -10,
                tilt_y: 20,
            }
        );

        // 90°: X' = H - Y = 25000, Y' = X = 10000, TiltX' = -TiltY = -20, TiltY' = TiltX = -10
        assert_eq!(
            transform_absolute_coords(
                10000,
                5000,
                -10,
                20,
                w,
                h,
                5080,
                Orientation::Deg90,
                cal,
                area,
                screen,
                false
            ),
            TransformedStylusCoords::Absolute {
                x: 25000,
                y: 10000,
                tilt_x: -20,
                tilt_y: -10,
            }
        );

        // 180°: X' = W - X = 40000, Y' = H - Y = 25000, TiltX' = 10, TiltY' = -20
        assert_eq!(
            transform_absolute_coords(
                10000,
                5000,
                -10,
                20,
                w,
                h,
                5080,
                Orientation::Deg180,
                cal,
                area,
                screen,
                false
            ),
            TransformedStylusCoords::Absolute {
                x: 40000,
                y: 25000,
                tilt_x: 10,
                tilt_y: -20,
            }
        );

        // 270°: X' = Y = 5000, Y' = W - X = 40000, TiltX' = TiltY = 20, TiltY' = -TiltX = 10
        assert_eq!(
            transform_absolute_coords(
                10000,
                5000,
                -10,
                20,
                w,
                h,
                5080,
                Orientation::Deg270,
                cal,
                area,
                screen,
                false
            ),
            TransformedStylusCoords::Absolute {
                x: 5000,
                y: 40000,
                tilt_x: 20,
                tilt_y: 10,
            }
        );
    }

    #[test]
    fn active_area_clips_and_rescales() {
        let cal = ScreenCalibration::default();
        // Área de 5"x3" centrada (x=2.0", y=1.0", w=5.0", h=3.0") em mesa de 50800x30480 (10"x6" a 5080 LPI)
        let area = ActiveArea {
            x_hundredths_in: 200,
            y_hundredths_in: 100,
            w_hundredths_in: 500,
            h_hundredths_in: 300,
        };
        let screen = ScreenMapping::default();

        // Fora da área (x = 5000 < 2.0*5080 = 10160)
        assert_eq!(
            transform_absolute_coords(
                5000,
                10000,
                0,
                0,
                50800,
                30480,
                5080,
                Orientation::Deg0,
                cal,
                area,
                screen,
                false
            ),
            TransformedStylusCoords::OutsideActiveArea
        );

        // No centro da área ativa (x = 10160 + 12700 = 22860) -> metade do eixo lógico (25400)
        assert_eq!(
            transform_absolute_coords(
                22860,
                12700,
                0,
                0,
                50800,
                30480,
                5080,
                Orientation::Deg0,
                cal,
                area,
                screen,
                false
            ),
            TransformedStylusCoords::Absolute {
                x: 25400,
                y: 15240,
                tilt_x: 0,
                tilt_y: 0,
            }
        );
    }

    #[test]
    fn relative_mode_accumulates_remainder() {
        let mut rel = RelativeAccumulator::default();
        // Speed = 5 -> denom = 5 * (10 - 5) = 25
        assert_eq!(rel.update(100, 100, 5), (0, 0));
        // Delta bruto = +20 -> < 25 -> retorna 0 mas preserva rem_x = 20
        assert_eq!(rel.update(120, 100, 5), (0, 0));
        // Delta bruto = +15 -> 20 + 15 = 35 -> dx = 1, rem_x = 10
        assert_eq!(rel.update(135, 100, 5), (1, 0));
    }

    #[test]
    fn pressure_curve_linear_soft_and_disabled() {
        let linear = PressureCurveLut::build(&PressureCurveParams::default(), 8191, false);
        assert_eq!(linear.map(0), 0);
        assert_eq!(linear.map(4095), 4095);
        assert_eq!(linear.map(8191), 8191);

        // Curva macia (soft): p1y = 0.8 -> meio da pressão produz valor > 5000
        let soft = PressureCurveLut::build(
            &PressureCurveParams {
                p0: 0.0,
                p1x: 0.5,
                p1y: 0.8,
                p2: 1.0,
                points: Vec::new(),
            },
            8191,
            false,
        );
        assert!(soft.map(4095) > 5000);

        // Disable pressure: qualquer valor > 0 vira máximo
        let disabled = PressureCurveLut::build(&PressureCurveParams::default(), 8191, true);
        assert_eq!(disabled.map(0), 0);
        assert_eq!(disabled.map(10), 8191);

        // Multi-pontos (spline monótona) passa exatamente pelos nós de controle
        let multi = PressureCurveLut::build(
            &PressureCurveParams {
                p0: 0.0,
                p1x: 0.25,
                p1y: 0.5,
                p2: 1.0,
                points: vec![
                    CurveControlPoint { x: 0.25, y: 0.5 },
                    CurveControlPoint { x: 0.75, y: 0.9 },
                ],
            },
            8000,
            false,
        );
        assert_eq!(multi.map(2000), 4000);
        assert_eq!(multi.map(6000), 7200);
    }
}
