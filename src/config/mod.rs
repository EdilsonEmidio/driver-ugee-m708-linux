pub mod profiles;
pub mod transform;
pub mod xml_import;

use crate::error::{DriverError, DriverResult};
use profiles::Action;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use transform::{
    ActiveArea, Orientation, PressureCurveLut, PressureCurveParams, ScreenCalibration,
    ScreenMapping,
};

/// Configuração dos botões físicos da caneta (`PenBtn0`=Ponta opcional, `PenBtn1`, `PenBtn2`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PenButtonsConfig {
    pub barrel_1: Action,
    pub barrel_2: Action,
}

impl Default for PenButtonsConfig {
    fn default() -> Self {
        Self {
            // Padrão UGEE XML: PenBtn1 = Alternar Caneta/Borracha (`Actid=1`) ou Botão Direito (`207`)
            // PenBtn1 default em M708 é Botão Direito (207) ou Pen/Eraser (1); usamos Botão Direito + Middle/Eraser amigáveis.
            barrel_1: Action::from_actid(207, None),
            barrel_2: Action::from_actid(1, None),
        }
    }
}

/// Configuração de um anel / Touch Ring (`W1..W4`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RingConfig {
    pub cw: Action,
    pub ccw: Action,
}

impl Default for RingConfig {
    fn default() -> Self {
        Self {
            cw: Action::from_actid(321, None),  // Ctrl++ (Zoom In)
            ccw: Action::from_actid(322, None), // Ctrl+- (Zoom Out)
        }
    }
}

/// Configuração completa do driver user-space (serializável em TOML).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DriverConfig {
    pub orientation: Orientation,
    pub disable_pressure: bool,
    pub disable_tilt: bool,
    pub disable_express_keys: bool,
    pub precision_mode: bool,
    pub osd_notifications: bool,
    pub relative_mode: bool,
    pub mouse_speed: u8,
    pub active_area: ActiveArea,
    pub screen_mapping: ScreenMapping,
    pub calibration: ScreenCalibration,
    pub pressure_curve: PressureCurveParams,
    pub pen_buttons: PenButtonsConfig,
    /// Mapeamento da tecla expressa (`"k1"` = bit 0 .. `"k40"` = bit 39) para a ação configurada.
    pub express_keys: BTreeMap<String, Action>,
    /// Rótulos amigáveis das teclas expressas (`"k1"` .. `"k40"`) para exibição na GUI e OSD.
    pub express_key_labels: BTreeMap<String, String>,
    /// Ações do anel 1 (`cw`/`ccw`).
    pub ring_1: RingConfig,
    /// Ações do anel 2 (`cw`/`ccw`).
    pub ring_2: RingConfig,
    /// Modos alternáveis adicionais do anel 1 (`W1..W4`, onde `W1` sincroniza com `ring_1`).
    pub ring_1_modes: Vec<RingConfig>,
    /// Modos alternáveis adicionais do anel 2 (`W1..W4`, onde `W1` sincroniza com `ring_2`).
    pub ring_2_modes: Vec<RingConfig>,
}

impl Default for DriverConfig {
    fn default() -> Self {
        // Mapeamento padrão K1..K8 compatível com UGEE M708 (`Ugee_Tablet.xml` <M708>)
        let mut express_keys = BTreeMap::new();
        express_keys.insert("k1".to_string(), Action::from_actid(301, None)); // K1: B (Brush)
        express_keys.insert("k2".to_string(), Action::from_actid(302, None)); // K2: E (Eraser)
        express_keys.insert("k3".to_string(), Action::from_actid(204, None)); // K3: Ctrl
        express_keys.insert("k4".to_string(), Action::from_actid(205, None)); // K4: Space
        express_keys.insert("k5".to_string(), Action::from_actid(306, None)); // K5: Ctrl+Z (Undo)
        express_keys.insert("k6".to_string(), Action::from_actid(321, None)); // K6: Ctrl++ (Zoom In)
        express_keys.insert("k7".to_string(), Action::from_actid(322, None)); // K7: Ctrl+- (Zoom Out)
        express_keys.insert("k8".to_string(), Action::from_actid(305, None)); // K8: Ctrl+S (Save)

        let mut express_key_labels = BTreeMap::new();
        express_key_labels.insert("k1".to_string(), "Pincel (B)".to_string());
        express_key_labels.insert("k2".to_string(), "Borracha (E)".to_string());
        express_key_labels.insert("k3".to_string(), "Modificador Ctrl".to_string());
        express_key_labels.insert("k4".to_string(), "Mão / Pan (Espaço)".to_string());
        express_key_labels.insert("k5".to_string(), "Desfazer (Ctrl+Z)".to_string());
        express_key_labels.insert("k6".to_string(), "Zoom + (Ctrl++)".to_string());
        express_key_labels.insert("k7".to_string(), "Zoom - (Ctrl+-)".to_string());
        express_key_labels.insert("k8".to_string(), "Salvar (Ctrl+S)".to_string());

        Self {
            orientation: Orientation::Deg0,
            disable_pressure: false,
            disable_tilt: false,
            disable_express_keys: false,
            precision_mode: false,
            osd_notifications: true,
            relative_mode: false,
            mouse_speed: 5,
            active_area: ActiveArea::default(),
            screen_mapping: ScreenMapping::default(),
            calibration: ScreenCalibration::default(),
            pressure_curve: PressureCurveParams::default(),
            pen_buttons: PenButtonsConfig::default(),
            express_keys,
            express_key_labels,
            ring_1: RingConfig::default(),
            ring_2: RingConfig {
                cw: Action::from_actid(210, None),
                ccw: Action::from_actid(211, None),
            },
            ring_1_modes: vec![
                RingConfig::default(),
                RingConfig {
                    cw: Action::from_actid(325, None),  // ] (Pincel maior)
                    ccw: Action::from_actid(326, None), // [ (Pincel menor)
                },
                RingConfig {
                    cw: Action::from_actid(210, None),  // Scroll Up
                    ccw: Action::from_actid(211, None), // Scroll Down
                },
                RingConfig {
                    cw: Action::from_actid(704, None),  // Vol +
                    ccw: Action::from_actid(705, None), // Vol -
                },
            ],
            ring_2_modes: vec![RingConfig {
                cw: Action::from_actid(210, None),
                ccw: Action::from_actid(211, None),
            }],
        }
    }
}

impl DriverConfig {
    /// Caminho padrão XDG para persistir a configuração do usuário (`~/.config/ugee-driver/config.toml`).
    pub fn default_config_path() -> PathBuf {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            let trimmed = xdg.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed).join("ugee-driver/config.toml");
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            let trimmed = home.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed).join(".config/ugee-driver/config.toml");
            }
        }
        PathBuf::from("ugee-config.toml")
    }

    /// Carrega um arquivo TOML do caminho informado.
    pub fn load_from_path(path: &Path) -> DriverResult<Self> {
        let raw = fs::read_to_string(path).map_err(|source| DriverError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&raw).map_err(|err| DriverError::Config {
            path: path.to_path_buf(),
            reason: err.to_string(),
        })
    }

    /// Grava a configuração atual em formato TOML criando o diretório pai se necessário.
    pub fn save_to_path(&self, path: &Path) -> DriverResult<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|source| DriverError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
        }
        let content = self.to_toml_string()?;
        fs::write(path, content).map_err(|source| DriverError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Serializa a configuração atual em formato TOML legível.
    pub fn to_toml_string(&self) -> DriverResult<String> {
        toml::to_string_pretty(self).map_err(|err| DriverError::Config {
            path: Path::new("<memory>").to_path_buf(),
            reason: err.to_string(),
        })
    }

    /// Retorna a ação associada ao bit 0-based da Express Key (ex.: bit `0` -> `"k1"` ou `"0"`).
    pub fn express_key_action(&self, bit_index: usize) -> Option<&Action> {
        let named = format!("k{}", bit_index + 1);
        self.express_keys
            .get(&named)
            .or_else(|| self.express_keys.get(&bit_index.to_string()))
    }

    /// Retorna o rótulo amigável da Express Key (`"k1"`..`"k40"`) com fallback na descrição da ação.
    pub fn express_key_label(&self, bit_index: usize) -> String {
        let named = format!("k{}", bit_index + 1);
        if let Some(lbl) = self.express_key_labels.get(&named) {
            let trimmed = lbl.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        self.express_key_action(bit_index)
            .map(Action::describe)
            .unwrap_or_else(|| format!("K{}", bit_index + 1))
    }

    /// Retorna a quantidade de modos configurados para o anel (`ring_index`: `0` ou `1`).
    pub fn ring_mode_count(&self, ring_index: u8) -> usize {
        let modes = if ring_index == 0 {
            &self.ring_1_modes
        } else {
            &self.ring_2_modes
        };
        modes.len().clamp(1, 4)
    }

    /// Retorna o par de ações `(cw/ccw)` ativo para o anel e slot `W1..W4` (`0..=3`).
    pub fn ring_mode_at(&self, ring_index: u8, mode_idx: usize) -> &RingConfig {
        let (fallback, modes) = if ring_index == 0 {
            (&self.ring_1, &self.ring_1_modes)
        } else {
            (&self.ring_2, &self.ring_2_modes)
        };
        if mode_idx == 0 {
            return fallback;
        }
        modes.get(mode_idx).unwrap_or(fallback)
    }

    /// Constrói a LUT de curva de pressão para a escala de pressão máxima da mesa conectada.
    pub fn build_pressure_lut(&self, max_pressure: u16) -> PressureCurveLut {
        PressureCurveLut::build(&self.pressure_curve, max_pressure, self.disable_pressure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_default_toml_config() -> Result<(), Box<dyn std::error::Error>> {
        let cfg = DriverConfig::default();
        let text = cfg.to_toml_string()?;
        let parsed: DriverConfig = toml::from_str(&text)?;
        assert_eq!(parsed.orientation, cfg.orientation);
        assert_eq!(parsed.mouse_speed, 5);
        assert_eq!(parsed.express_key_action(4), cfg.express_key_action(4));
        Ok(())
    }
}
