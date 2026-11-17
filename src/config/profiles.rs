use evdev::Key;
use serde::{Deserialize, Serialize};

/// Ação executável associada a um botão da caneta, Express Key ou direção do Touch Ring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Nenhuma ação (`Actid = 10`).
    None,
    /// Alterna o modo da ponta entre Caneta e Borracha (`Actid = 1` ou `409`).
    ToggleEraser,
    /// Alterna entre os modos de função do Touch Ring (`Actid = 405`).
    CycleRingMode,
    /// Alterna precisão espacial reduzida (`Actid = 404`).
    TogglePrecisionMode,
    /// Alterna entre tecla B (Brush) e E (Eraser) (`Actid = 408`).
    ToggleBrushEraserKeys,
    /// Emite clique do botão esquerdo do mouse (`Actid = 206`).
    MouseLeft,
    /// Emite clique do botão direito do mouse (`Actid = 207`).
    MouseRight,
    /// Emite clique do botão do meio / scroll (`Actid = 208`).
    MouseMiddle,
    /// Emite duplo clique esquerdo (`Actid = 209`).
    MouseDoubleLeft,
    /// Emite rolagem vertical (`+1` para cima / `Actid = 210`, `-1` para baixo / `Actid = 211`).
    MouseWheel { delta: i32 },
    /// Emite uma combinação simultânea de teclas Linux `evdev` (ex: `[KEY_LEFTCTRL, KEY_Z]`).
    KeyCombo { keys: Vec<u16> },
    /// Emite botão nativo de mesa (`BTN_0 + index`) para integração Wayland/libwacom.
    TabletButton { index: u8 },
    /// Executa um comando externo em background.
    RunCommand { command: String },
    /// Abre a interface gráfica de configuração (`ugee-gui`).
    OpenGui,
}

impl Action {
    /// Constrói uma ação de combinação de teclas a partir de `evdev::Key`.
    pub fn combo(keys: &[Key]) -> Self {
        Self::KeyCombo {
            keys: keys.iter().map(|k| k.code()).collect(),
        }
    }

    /// Resolve um `Actid` numérico oficial (`Ugee_Tablet.xml` / `language.ini`) para `Action`.
    pub fn from_actid(actid: u16, custom_keys: Option<&[Key]>) -> Self {
        match actid {
            1 | 409 => Self::ToggleEraser,
            10 => Self::None,
            11 => match custom_keys {
                Some(keys) if !keys.is_empty() => Self::combo(keys),
                _ => Self::None,
            },
            201 => Self::combo(&[Key::KEY_LEFTSHIFT]),
            202 => Self::combo(&[Key::KEY_LEFTALT]),
            203 => Self::combo(&[Key::KEY_RIGHTALT]),
            204 => Self::combo(&[Key::KEY_LEFTCTRL]),
            205 | 304 => Self::combo(&[Key::KEY_SPACE]),
            206 => Self::MouseLeft,
            207 => Self::MouseRight,
            208 => Self::MouseMiddle,
            209 => Self::MouseDoubleLeft,
            210 => Self::MouseWheel { delta: 1 },
            211 => Self::MouseWheel { delta: -1 },
            301 => Self::combo(&[Key::KEY_B]),
            302 => Self::combo(&[Key::KEY_E]),
            303 => Self::combo(&[Key::KEY_LEFTALT]),
            305 => Self::combo(&[Key::KEY_LEFTCTRL, Key::KEY_S]),
            306 => Self::combo(&[Key::KEY_LEFTCTRL, Key::KEY_Z]),
            307 => Self::combo(&[Key::KEY_LEFTCTRL, Key::KEY_LEFTALT, Key::KEY_Z]),
            308 => Self::combo(&[Key::KEY_LEFTCTRL, Key::KEY_LEFTSHIFT, Key::KEY_Z]),
            309 => Self::combo(&[Key::KEY_V]),
            310 => Self::combo(&[Key::KEY_L]),
            311 => Self::combo(&[Key::KEY_LEFTCTRL, Key::KEY_O]),
            312 => Self::combo(&[Key::KEY_LEFTCTRL, Key::KEY_N]),
            313 => Self::combo(&[Key::KEY_LEFTCTRL, Key::KEY_LEFTSHIFT, Key::KEY_N]),
            314 => Self::combo(&[Key::KEY_LEFTCTRL, Key::KEY_E]),
            315 => Self::combo(&[Key::KEY_F]),
            316 => Self::combo(&[Key::KEY_D]),
            317 => Self::combo(&[Key::KEY_X]),
            318 => Self::combo(&[Key::KEY_LEFTCTRL, Key::KEY_DELETE]),
            319 => Self::combo(&[Key::KEY_LEFTCTRL, Key::KEY_C]),
            320 => Self::combo(&[Key::KEY_LEFTCTRL, Key::KEY_V]),
            321 => Self::combo(&[Key::KEY_LEFTCTRL, Key::KEY_EQUAL]),
            322 => Self::combo(&[Key::KEY_LEFTCTRL, Key::KEY_MINUS]),
            323 => Self::combo(&[Key::KEY_TAB]),
            324 => Self::combo(&[Key::KEY_F5]),
            325 => Self::combo(&[Key::KEY_RIGHTBRACE]),
            326 => Self::combo(&[Key::KEY_LEFTBRACE]),
            327 => Self::combo(&[Key::KEY_LEFTCTRL, Key::KEY_RIGHTBRACE]),
            328 => Self::combo(&[Key::KEY_LEFTCTRL, Key::KEY_LEFTBRACE]),
            404 => Self::TogglePrecisionMode,
            405 | 406 => Self::CycleRingMode,
            408 => Self::ToggleBrushEraserKeys,
            604 => Self::combo(&[Key::KEY_LEFTMETA, Key::KEY_TAB]),
            605 => Self::combo(&[Key::KEY_LEFTMETA]),
            701 => Self::combo(&[Key::KEY_PREVIOUSSONG]),
            702 => Self::combo(&[Key::KEY_NEXTSONG]),
            703 => Self::combo(&[Key::KEY_PLAYPAUSE]),
            704 => Self::combo(&[Key::KEY_VOLUMEUP]),
            705 => Self::combo(&[Key::KEY_VOLUMEDOWN]),
            706 => Self::combo(&[Key::KEY_MUTE]),
            _ => Self::None,
        }
    }

    /// Retorna uma descrição legível da ação para exibição na GUI/CLI.
    pub fn describe(&self) -> String {
        if let Some(preset) = ACTION_PRESETS.iter().find(|p| &p.action() == self) {
            return preset.label.to_string();
        }
        match self {
            Self::None => "Nenhuma função".to_string(),
            Self::ToggleEraser => "Alternar Caneta / Borracha".to_string(),
            Self::CycleRingMode => "Alternar modo do Anel".to_string(),
            Self::TogglePrecisionMode => "Modo de precisão".to_string(),
            Self::ToggleBrushEraserKeys => "Alternar teclas [B] / [E]".to_string(),
            Self::MouseLeft => "Clique Esquerdo".to_string(),
            Self::MouseRight => "Clique Direito".to_string(),
            Self::MouseMiddle => "Clique do Meio".to_string(),
            Self::MouseDoubleLeft => "Duplo Clique Esquerdo".to_string(),
            Self::MouseWheel { delta } if *delta > 0 => "Scroll Cima".to_string(),
            Self::MouseWheel { .. } => "Scroll Baixo".to_string(),
            Self::KeyCombo { .. } => self
                .to_combo_str()
                .unwrap_or_else(|| "Combo vazio".to_string()),
            Self::TabletButton { index } => format!("Botão Nativo Wayland (BTN_{index})"),
            Self::RunCommand { command } => {
                if command.trim().is_empty() {
                    "Executar Aplicativo".to_string()
                } else {
                    format!("Executar: {}", command.trim())
                }
            }
            Self::OpenGui => "Abrir Painel Ugee".to_string(),
        }
    }

    /// Converte uma ação `KeyCombo` em string reversível compatível com `parse_key_combo_str`.
    /// Retorna `None` para ações que não são combinações de teclas.
    pub fn to_combo_str(&self) -> Option<String> {
        let Self::KeyCombo { keys } = self else {
            return None;
        };
        if keys.is_empty() {
            return None;
        }
        let formatted: Vec<String> = keys
            .iter()
            .map(|&code| format_single_key_name(Key::new(code)))
            .collect();
        Some(formatted.join("+"))
    }

    /// Faz o parsing de uma string amigável de atalho (ex.: `"Ctrl+Shift+Z"`, `"B"`, `"Space"`).
    pub fn parse_key_combo_str(input: &str) -> Option<Self> {
        let parts: Vec<&str> = input
            .split('+')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if parts.is_empty() {
            return None;
        }
        let mut keys = Vec::with_capacity(parts.len());
        for part in parts {
            let key = parse_single_key_name(part)?;
            keys.push(key);
        }
        Some(Self::combo(&keys))
    }
}

/// Item de catálogo nomeado usado nos seletores da interface gráfica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub actid: u16,
}

impl ActionPreset {
    pub fn action(self) -> Action {
        Action::from_actid(self.actid, None)
    }
}

/// Catálogo de ações pré-definidas expostas nos menus da GUI `egui`.
pub const ACTION_PRESETS: &[ActionPreset] = &[
    ActionPreset {
        id: "none",
        label: "Nenhuma função",
        actid: 10,
    },
    ActionPreset {
        id: "toggle_eraser",
        label: "Alternar Caneta / Borracha",
        actid: 1,
    },
    ActionPreset {
        id: "mouse_left",
        label: "Mouse: Clique Esquerdo",
        actid: 206,
    },
    ActionPreset {
        id: "mouse_right",
        label: "Mouse: Clique Direito",
        actid: 207,
    },
    ActionPreset {
        id: "mouse_middle",
        label: "Mouse: Clique do Meio",
        actid: 208,
    },
    ActionPreset {
        id: "mouse_double_left",
        label: "Mouse: Duplo Clique Esquerdo",
        actid: 209,
    },
    ActionPreset {
        id: "wheel_up",
        label: "Mouse: Scroll Cima",
        actid: 210,
    },
    ActionPreset {
        id: "wheel_down",
        label: "Mouse: Scroll Baixo",
        actid: 211,
    },
    ActionPreset {
        id: "key_b",
        label: "Tecla B (Pincel)",
        actid: 301,
    },
    ActionPreset {
        id: "key_e",
        label: "Tecla E (Borracha)",
        actid: 302,
    },
    ActionPreset {
        id: "toggle_b_e",
        label: "Alternar [B] / [E]",
        actid: 408,
    },
    ActionPreset {
        id: "precision",
        label: "Modo de Precisão",
        actid: 404,
    },
    ActionPreset {
        id: "key_ctrl",
        label: "Modificador Ctrl",
        actid: 204,
    },
    ActionPreset {
        id: "key_shift",
        label: "Modificador Shift",
        actid: 201,
    },
    ActionPreset {
        id: "key_alt",
        label: "Modificador Alt",
        actid: 202,
    },
    ActionPreset {
        id: "key_space",
        label: "Espaço (Pan/Hand)",
        actid: 205,
    },
    ActionPreset {
        id: "ctrl_z",
        label: "Desfazer (Ctrl+Z)",
        actid: 306,
    },
    ActionPreset {
        id: "ctrl_shift_z",
        label: "Refazer (Ctrl+Shift+Z)",
        actid: 308,
    },
    ActionPreset {
        id: "ctrl_alt_z",
        label: "Histórico (Ctrl+Alt+Z)",
        actid: 307,
    },
    ActionPreset {
        id: "ctrl_s",
        label: "Salvar (Ctrl+S)",
        actid: 305,
    },
    ActionPreset {
        id: "ctrl_c",
        label: "Copiar (Ctrl+C)",
        actid: 319,
    },
    ActionPreset {
        id: "ctrl_v",
        label: "Colar (Ctrl+V)",
        actid: 320,
    },
    ActionPreset {
        id: "ctrl_o",
        label: "Abrir (Ctrl+O)",
        actid: 311,
    },
    ActionPreset {
        id: "ctrl_n",
        label: "Novo (Ctrl+N)",
        actid: 312,
    },
    ActionPreset {
        id: "zoom_in",
        label: "Zoom + (Ctrl+=)",
        actid: 321,
    },
    ActionPreset {
        id: "zoom_out",
        label: "Zoom - (Ctrl+-)",
        actid: 322,
    },
    ActionPreset {
        id: "brush_up",
        label: "Pincel Maior (])",
        actid: 325,
    },
    ActionPreset {
        id: "brush_down",
        label: "Pincel Menor ([)",
        actid: 326,
    },
    ActionPreset {
        id: "tab",
        label: "Tab (Ocultar Painéis)",
        actid: 323,
    },
    ActionPreset {
        id: "f5",
        label: "F5",
        actid: 324,
    },
    ActionPreset {
        id: "media_play",
        label: "Mídia: Play/Pause",
        actid: 703,
    },
    ActionPreset {
        id: "media_vol_up",
        label: "Mídia: Volume +",
        actid: 704,
    },
    ActionPreset {
        id: "media_vol_down",
        label: "Mídia: Volume -",
        actid: 705,
    },
    ActionPreset {
        id: "media_mute",
        label: "Mídia: Mudo",
        actid: 706,
    },
];

fn format_single_key_name(key: Key) -> String {
    match key {
        Key::KEY_LEFTCTRL => "Ctrl".to_string(),
        Key::KEY_RIGHTCTRL => "RCtrl".to_string(),
        Key::KEY_LEFTSHIFT => "Shift".to_string(),
        Key::KEY_RIGHTSHIFT => "RShift".to_string(),
        Key::KEY_LEFTALT => "Alt".to_string(),
        Key::KEY_RIGHTALT => "RAlt".to_string(),
        Key::KEY_LEFTMETA => "Super".to_string(),
        Key::KEY_SPACE => "Space".to_string(),
        Key::KEY_TAB => "Tab".to_string(),
        Key::KEY_ESC => "Esc".to_string(),
        Key::KEY_ENTER => "Enter".to_string(),
        Key::KEY_BACKSPACE => "Backspace".to_string(),
        Key::KEY_DELETE => "Delete".to_string(),
        Key::KEY_LEFTBRACE => "[".to_string(),
        Key::KEY_RIGHTBRACE => "]".to_string(),
        Key::KEY_EQUAL => "=".to_string(),
        Key::KEY_MINUS => "-".to_string(),
        other => format!("{other:?}").trim_start_matches("KEY_").to_string(),
    }
}

fn parse_single_key_name(raw: &str) -> Option<Key> {
    match raw.to_ascii_uppercase().as_str() {
        "CTRL" | "CONTROL" | "LEFTCTRL" | "LCTRL" => Some(Key::KEY_LEFTCTRL),
        "RCTRL" | "RIGHTCTRL" => Some(Key::KEY_RIGHTCTRL),
        "SHIFT" | "LEFTSHIFT" | "LSHIFT" => Some(Key::KEY_LEFTSHIFT),
        "RSHIFT" | "RIGHTSHIFT" => Some(Key::KEY_RIGHTSHIFT),
        "ALT" | "LEFTALT" | "LALT" => Some(Key::KEY_LEFTALT),
        "RALT" | "RIGHTALT" | "ALTGR" => Some(Key::KEY_RIGHTALT),
        "SUPER" | "META" | "WIN" | "LEFTMETA" => Some(Key::KEY_LEFTMETA),
        "SPACE" | "ESPACO" => Some(Key::KEY_SPACE),
        "TAB" => Some(Key::KEY_TAB),
        "ESC" | "ESCAPE" => Some(Key::KEY_ESC),
        "ENTER" | "RETURN" => Some(Key::KEY_ENTER),
        "BACKSPACE" => Some(Key::KEY_BACKSPACE),
        "DEL" | "DELETE" => Some(Key::KEY_DELETE),
        "UP" => Some(Key::KEY_UP),
        "DOWN" => Some(Key::KEY_DOWN),
        "LEFT" => Some(Key::KEY_LEFT),
        "RIGHT" => Some(Key::KEY_RIGHT),
        "HOME" => Some(Key::KEY_HOME),
        "END" => Some(Key::KEY_END),
        "PAGEUP" => Some(Key::KEY_PAGEUP),
        "PAGEDOWN" => Some(Key::KEY_PAGEDOWN),
        "F1" => Some(Key::KEY_F1),
        "F2" => Some(Key::KEY_F2),
        "F3" => Some(Key::KEY_F3),
        "F4" => Some(Key::KEY_F4),
        "F5" => Some(Key::KEY_F5),
        "F6" => Some(Key::KEY_F6),
        "F7" => Some(Key::KEY_F7),
        "F8" => Some(Key::KEY_F8),
        "F9" => Some(Key::KEY_F9),
        "F10" => Some(Key::KEY_F10),
        "F11" => Some(Key::KEY_F11),
        "F12" => Some(Key::KEY_F12),
        "[" | "LEFTBRACE" => Some(Key::KEY_LEFTBRACE),
        "]" | "RIGHTBRACE" => Some(Key::KEY_RIGHTBRACE),
        "=" | "+" | "EQUAL" => Some(Key::KEY_EQUAL),
        "-" | "MINUS" => Some(Key::KEY_MINUS),
        "0" => Some(Key::KEY_0),
        "1" => Some(Key::KEY_1),
        "2" => Some(Key::KEY_2),
        "3" => Some(Key::KEY_3),
        "4" => Some(Key::KEY_4),
        "5" => Some(Key::KEY_5),
        "6" => Some(Key::KEY_6),
        "7" => Some(Key::KEY_7),
        "8" => Some(Key::KEY_8),
        "9" => Some(Key::KEY_9),
        "A" => Some(Key::KEY_A),
        "B" => Some(Key::KEY_B),
        "C" => Some(Key::KEY_C),
        "D" => Some(Key::KEY_D),
        "E" => Some(Key::KEY_E),
        "F" => Some(Key::KEY_F),
        "G" => Some(Key::KEY_G),
        "H" => Some(Key::KEY_H),
        "I" => Some(Key::KEY_I),
        "J" => Some(Key::KEY_J),
        "K" => Some(Key::KEY_K),
        "L" => Some(Key::KEY_L),
        "M" => Some(Key::KEY_M),
        "N" => Some(Key::KEY_N),
        "O" => Some(Key::KEY_O),
        "P" => Some(Key::KEY_P),
        "Q" => Some(Key::KEY_Q),
        "R" => Some(Key::KEY_R),
        "S" => Some(Key::KEY_S),
        "T" => Some(Key::KEY_T),
        "U" => Some(Key::KEY_U),
        "V" => Some(Key::KEY_V),
        "W" => Some(Key::KEY_W),
        "X" => Some(Key::KEY_X),
        "Y" => Some(Key::KEY_Y),
        "Z" => Some(Key::KEY_Z),
        "PREVIOUSSONG" => Some(Key::KEY_PREVIOUSSONG),
        "NEXTSONG" => Some(Key::KEY_NEXTSONG),
        "PLAYPAUSE" => Some(Key::KEY_PLAYPAUSE),
        "VOLUMEUP" => Some(Key::KEY_VOLUMEUP),
        "VOLUMEDOWN" => Some(Key::KEY_VOLUMEDOWN),
        "MUTE" => Some(Key::KEY_MUTE),
        _ => None,
    }
}

/// Retorna todas as teclas `evdev::Key` necessárias para registrar no `VirtualPad`.
pub fn supported_pad_keys() -> Vec<Key> {
    vec![
        Key::KEY_ESC,
        Key::KEY_TAB,
        Key::KEY_SPACE,
        Key::KEY_ENTER,
        Key::KEY_BACKSPACE,
        Key::KEY_DELETE,
        Key::KEY_LEFTCTRL,
        Key::KEY_RIGHTCTRL,
        Key::KEY_LEFTSHIFT,
        Key::KEY_RIGHTSHIFT,
        Key::KEY_LEFTALT,
        Key::KEY_RIGHTALT,
        Key::KEY_LEFTMETA,
        Key::KEY_UP,
        Key::KEY_DOWN,
        Key::KEY_LEFT,
        Key::KEY_RIGHT,
        Key::KEY_HOME,
        Key::KEY_END,
        Key::KEY_PAGEUP,
        Key::KEY_PAGEDOWN,
        Key::KEY_0,
        Key::KEY_1,
        Key::KEY_2,
        Key::KEY_3,
        Key::KEY_4,
        Key::KEY_5,
        Key::KEY_6,
        Key::KEY_7,
        Key::KEY_8,
        Key::KEY_9,
        Key::KEY_A,
        Key::KEY_B,
        Key::KEY_C,
        Key::KEY_D,
        Key::KEY_E,
        Key::KEY_F,
        Key::KEY_G,
        Key::KEY_H,
        Key::KEY_I,
        Key::KEY_J,
        Key::KEY_K,
        Key::KEY_L,
        Key::KEY_M,
        Key::KEY_N,
        Key::KEY_O,
        Key::KEY_P,
        Key::KEY_Q,
        Key::KEY_R,
        Key::KEY_S,
        Key::KEY_T,
        Key::KEY_U,
        Key::KEY_V,
        Key::KEY_W,
        Key::KEY_X,
        Key::KEY_Y,
        Key::KEY_Z,
        Key::KEY_EQUAL,
        Key::KEY_MINUS,
        Key::KEY_LEFTBRACE,
        Key::KEY_RIGHTBRACE,
        Key::KEY_F1,
        Key::KEY_F2,
        Key::KEY_F3,
        Key::KEY_F4,
        Key::KEY_F5,
        Key::KEY_F6,
        Key::KEY_F7,
        Key::KEY_F8,
        Key::KEY_F9,
        Key::KEY_F10,
        Key::KEY_F11,
        Key::KEY_F12,
        Key::KEY_PREVIOUSSONG,
        Key::KEY_NEXTSONG,
        Key::KEY_PLAYPAUSE,
        Key::KEY_VOLUMEUP,
        Key::KEY_VOLUMEDOWN,
        Key::KEY_MUTE,
        Key::BTN_LEFT,
        Key::BTN_RIGHT,
        Key::BTN_MIDDLE,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_actid_shortcuts_accurately() {
        assert_eq!(Action::from_actid(1, None), Action::ToggleEraser);
        assert_eq!(Action::from_actid(10, None), Action::None);
        assert_eq!(Action::from_actid(207, None), Action::MouseRight);
        assert_eq!(
            Action::from_actid(306, None),
            Action::combo(&[Key::KEY_LEFTCTRL, Key::KEY_Z])
        );
        assert_eq!(
            Action::from_actid(308, None),
            Action::combo(&[Key::KEY_LEFTCTRL, Key::KEY_LEFTSHIFT, Key::KEY_Z])
        );
        assert_eq!(
            Action::from_actid(704, None),
            Action::combo(&[Key::KEY_VOLUMEUP])
        );
    }

    #[test]
    fn parses_all_alphabet_digits_fkeys_and_roundtrips_presets() {
        let supported: std::collections::BTreeSet<u16> =
            supported_pad_keys().into_iter().map(|k| k.code()).collect();

        for name in [
            "Ctrl+A",
            "G",
            "H",
            "J",
            "Q",
            "0",
            "9",
            "F1",
            "F12",
            "Ctrl+Shift+Z",
            "Space",
            "[",
            "]",
        ] {
            let parsed = Action::parse_key_combo_str(name);
            assert!(parsed.is_some(), "falha ao interpretar '{name}'");
            if let Some(Action::KeyCombo { keys }) = parsed {
                for code in keys {
                    assert!(
                        supported.contains(&code),
                        "tecla {code} de '{name}' ausente em supported_pad_keys()"
                    );
                }
            }
        }

        for preset in ACTION_PRESETS {
            let act = preset.action();
            if matches!(act, Action::KeyCombo { .. }) {
                let text = act.to_combo_str();
                assert!(text.is_some(), "preset {} sem to_combo_str", preset.id);
                if let Some(ref combo) = text {
                    assert_eq!(Action::parse_key_combo_str(combo), Some(act));
                }
            } else {
                assert_eq!(act.to_combo_str(), None);
            }
        }
    }
}
