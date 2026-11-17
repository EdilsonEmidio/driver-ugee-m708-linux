/// Vendor ID USB oficial da Hanvon Ugee / UGEE / XP-Pen.
pub const UGEE_VENDOR_ID: u16 = 0x28BD;

/// Quirks específicos por PID observados no disassembly de `ugeeTabletDriver.exe`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PidQuirks {
    /// `0x0074`: inverte a direção CW/CCW na decodificação em quadratura do dial.
    pub invert_ring_quadrature: bool,
    /// `0x0201`: formato legado de Express Keys com 1 byte por tecla.
    pub legacy_1byte_express_keys: bool,
    /// `0x0933` / `0x0934`: consulta dupla de brilho dos LEDs no evento de wake (`cmd=0`).
    pub dual_led_query_on_wake: bool,
}

/// Descrição estática de um PID mapeado em `name_config.ini` ou descoberto em hardware real.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownPid {
    pub pid: u16,
    pub name: &'static str,
    pub xml_profile: &'static str,
}

/// Perfil base extraído dos 34 modelos de `Ugee_Tablet.xml` (v1.4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelProfile {
    pub name: &'static str,
    pub ring_num: u8,
    pub key_num: u8,
    pub pen_type: u8,
    pub trackpad: u8,
    pub control: u8,
    pub has_calibration: bool,
    pub has_pen_btn0: bool,
    pub default_max_x: u32,
    pub default_max_y: u32,
    pub default_max_pressure: u16,
    pub default_lpi: u16,
}

/// Lista de PIDs conhecidos de `name_config.ini` e do disassembly + hardware validado (`0x0924`).
pub const KNOWN_PIDS: &[KnownPid] = &[
    KnownPid {
        pid: 0x000B,
        name: "UD13",
        xml_profile: "UD13",
    },
    KnownPid {
        pid: 0x0074,
        name: "UGEE Legacy Dial",
        xml_profile: "RB160",
    },
    KnownPid {
        pid: 0x0078,
        name: "Rainbow 160",
        xml_profile: "RB160",
    },
    KnownPid {
        pid: 0x0096,
        name: "EX12",
        xml_profile: "E12",
    },
    KnownPid {
        pid: 0x0201,
        name: "UGEE Legacy 1-Byte",
        xml_profile: "M708",
    },
    KnownPid {
        pid: 0x0803,
        name: "Rainbow 170",
        xml_profile: "RB160",
    },
    KnownPid {
        pid: 0x080A,
        name: "EXRAI Pro 12",
        xml_profile: "LPU1211F",
    },
    KnownPid {
        pid: 0x0905,
        name: "EX08",
        xml_profile: "BPU0908",
    },
    KnownPid {
        pid: 0x0907,
        name: "M708",
        xml_profile: "M708",
    },
    KnownPid {
        pid: 0x090A,
        name: "UG Pro 16",
        xml_profile: "LPU1611F",
    },
    KnownPid {
        pid: 0x090D,
        name: "EXRAI pro 16",
        xml_profile: "LPU1611F",
    },
    KnownPid {
        pid: 0x0918,
        name: "HS960",
        xml_profile: "M708",
    },
    KnownPid {
        pid: 0x0919,
        name: "S640-E",
        xml_profile: "BPH0908",
    },
    KnownPid {
        pid: 0x091A,
        name: "EXRAI um 16",
        xml_profile: "LPU1611F",
    },
    KnownPid {
        pid: 0x091B,
        name: "EXRAI Pro 22R",
        xml_profile: "LPU2202",
    },
    KnownPid {
        pid: 0x091D,
        name: "UG Pro 22",
        xml_profile: "LPU2202",
    },
    KnownPid {
        pid: 0x091E,
        name: "EXRAI pro 22",
        xml_profile: "LPU2202",
    },
    KnownPid {
        pid: 0x091F,
        name: "EXRAI 12",
        xml_profile: "LPU1211F",
    },
    KnownPid {
        pid: 0x0920,
        name: "H960",
        xml_profile: "M708",
    },
    KnownPid {
        pid: 0x0921,
        name: "S640W-E",
        xml_profile: "BPU0908",
    },
    KnownPid {
        pid: 0x0924,
        name: "M708",
        xml_profile: "M708",
    },
    KnownPid {
        pid: 0x0925,
        name: "EX08 Pro",
        xml_profile: "M808",
    },
    KnownPid {
        pid: 0x0927,
        name: "M708 v3",
        xml_profile: "M708",
    },
    KnownPid {
        pid: 0x0928,
        name: "EX08 S/SW",
        xml_profile: "M908",
    },
    KnownPid {
        pid: 0x092B,
        name: "EXRAI pro 13.3",
        xml_profile: "LPU1302F",
    },
    KnownPid {
        pid: 0x092C,
        name: "Y160F",
        xml_profile: "LPU1505F",
    },
    KnownPid {
        pid: 0x092D,
        name: "EXRAI Pro 24",
        xml_profile: "LPU2402Q",
    },
    KnownPid {
        pid: 0x0933,
        name: "UGEE Dual-LED",
        xml_profile: "M908",
    },
    KnownPid {
        pid: 0x0934,
        name: "UGEE Dual-LED",
        xml_profile: "M908",
    },
    KnownPid {
        pid: 0x093B,
        name: "UGEE U1200",
        xml_profile: "LPU1211F",
    },
    KnownPid {
        pid: 0x093C,
        name: "UGEE U1600",
        xml_profile: "LPU1611F",
    },
    KnownPid {
        pid: 0x0940,
        name: "EX08 Pro",
        xml_profile: "M808",
    },
    KnownPid {
        pid: 0x900A,
        name: "UG Pro 16",
        xml_profile: "LPU1611F",
    },
];

/// Perfis base dos modelos de `Ugee_Tablet.xml` (com dimensões fallback padrão).
pub const MODEL_PROFILES: &[ModelProfile] = &[
    ModelProfile {
        name: "M708",
        ring_num: 0,
        key_num: 8,
        pen_type: 0,
        trackpad: 0,
        control: 0,
        has_calibration: false,
        has_pen_btn0: true,
        default_max_x: 50794,
        default_max_y: 30474,
        default_max_pressure: 8191,
        default_lpi: 5080,
    },
    ModelProfile {
        name: "M808",
        ring_num: 0,
        key_num: 8,
        pen_type: 1,
        trackpad: 0,
        control: 0,
        has_calibration: false,
        has_pen_btn0: false,
        default_max_x: 50800,
        default_max_y: 31750,
        default_max_pressure: 8191,
        default_lpi: 5080,
    },
    ModelProfile {
        name: "M908",
        ring_num: 1,
        key_num: 8,
        pen_type: 1,
        trackpad: 0,
        control: 0,
        has_calibration: false,
        has_pen_btn0: false,
        default_max_x: 50800,
        default_max_y: 31750,
        default_max_pressure: 8191,
        default_lpi: 5080,
    },
    ModelProfile {
        name: "E12",
        ring_num: 0,
        key_num: 8,
        pen_type: 1,
        trackpad: 0,
        control: 0,
        has_calibration: true,
        has_pen_btn0: false,
        default_max_x: 50800,
        default_max_y: 28575,
        default_max_pressure: 8191,
        default_lpi: 5080,
    },
    ModelProfile {
        name: "E12PLUS",
        ring_num: 1,
        key_num: 8,
        pen_type: 1,
        trackpad: 0,
        control: 0,
        has_calibration: true,
        has_pen_btn0: false,
        default_max_x: 50800,
        default_max_y: 28575,
        default_max_pressure: 8191,
        default_lpi: 5080,
    },
    ModelProfile {
        name: "UE16",
        ring_num: 1,
        key_num: 9,
        pen_type: 15,
        trackpad: 0,
        control: 0,
        has_calibration: true,
        has_pen_btn0: true,
        default_max_x: 50800,
        default_max_y: 28575,
        default_max_pressure: 16383,
        default_lpi: 5080,
    },
    ModelProfile {
        name: "RB160",
        ring_num: 1,
        key_num: 6,
        pen_type: 0,
        trackpad: 0,
        control: 0,
        has_calibration: false,
        has_pen_btn0: false,
        default_max_x: 50800,
        default_max_y: 31750,
        default_max_pressure: 8191,
        default_lpi: 5080,
    },
    ModelProfile {
        name: "UD13",
        ring_num: 0,
        key_num: 6,
        pen_type: 0,
        trackpad: 0,
        control: 0,
        has_calibration: true,
        has_pen_btn0: false,
        default_max_x: 50800,
        default_max_y: 28575,
        default_max_pressure: 8191,
        default_lpi: 5080,
    },
    ModelProfile {
        name: "LPU1211F",
        ring_num: 0,
        key_num: 0,
        pen_type: 10,
        trackpad: 0,
        control: 0,
        has_calibration: true,
        has_pen_btn0: true,
        default_max_x: 50800,
        default_max_y: 28575,
        default_max_pressure: 8191,
        default_lpi: 5080,
    },
    ModelProfile {
        name: "LPU1302F",
        ring_num: 1,
        key_num: 8,
        pen_type: 12,
        trackpad: 0,
        control: 0,
        has_calibration: true,
        has_pen_btn0: false,
        default_max_x: 50800,
        default_max_y: 28575,
        default_max_pressure: 8191,
        default_lpi: 5080,
    },
    ModelProfile {
        name: "LPU1505F",
        ring_num: 2,
        key_num: 8,
        pen_type: 13,
        trackpad: 0,
        control: 0,
        has_calibration: true,
        has_pen_btn0: true,
        default_max_x: 50800,
        default_max_y: 28575,
        default_max_pressure: 8191,
        default_lpi: 5080,
    },
    ModelProfile {
        name: "LPU1611F",
        ring_num: 0,
        key_num: 0,
        pen_type: 10,
        trackpad: 0,
        control: 0,
        has_calibration: true,
        has_pen_btn0: true,
        default_max_x: 50800,
        default_max_y: 28575,
        default_max_pressure: 8191,
        default_lpi: 5080,
    },
    ModelProfile {
        name: "LPU2202",
        ring_num: 2,
        key_num: 20,
        pen_type: 12,
        trackpad: 0,
        control: 0,
        has_calibration: true,
        has_pen_btn0: false,
        default_max_x: 50800,
        default_max_y: 28575,
        default_max_pressure: 8191,
        default_lpi: 5080,
    },
    ModelProfile {
        name: "LPU2402Q",
        ring_num: 2,
        key_num: 20,
        pen_type: 12,
        trackpad: 0,
        control: 0,
        has_calibration: true,
        has_pen_btn0: false,
        default_max_x: 50800,
        default_max_y: 28575,
        default_max_pressure: 8191,
        default_lpi: 5080,
    },
];

/// Retorna os quirks específicos do PID informados pela engenharia reversa.
pub fn pid_quirks(pid: u16) -> PidQuirks {
    PidQuirks {
        invert_ring_quadrature: pid == 0x0074,
        legacy_1byte_express_keys: pid == 0x0201,
        dual_led_query_on_wake: matches!(pid, 0x0933 | 0x0934),
    }
}

/// Busca metadados estáticos de um PID conhecido.
pub fn lookup_pid(pid: u16) -> Option<&'static KnownPid> {
    KNOWN_PIDS.iter().find(|entry| entry.pid == pid)
}

/// Resolve o melhor perfil de modelo com base no PID, `ProductName` ou `ICVersion`.
///
/// Prefere o match de nome mais longo para evitar que prefixos (ex.: `"E12"`)
/// capturem nomes mais específicos (ex.: `"E12PLUS"`).
pub fn resolve_model_profile(
    pid: u16,
    product_name: Option<&str>,
    ic_version: Option<&str>,
) -> &'static ModelProfile {
    if let Some(ic) = ic_version {
        if let Some(best) = MODEL_PROFILES
            .iter()
            .filter(|p| ic.contains(p.name))
            .max_by_key(|p| p.name.len())
        {
            return best;
        }
    }
    if let Some(prod) = product_name {
        if let Some(best) = MODEL_PROFILES
            .iter()
            .filter(|p| prod.contains(p.name))
            .max_by_key(|p| p.name.len())
        {
            return best;
        }
    }
    if let Some(known) = lookup_pid(pid) {
        if let Some(profile) = MODEL_PROFILES.iter().find(|p| p.name == known.xml_profile) {
            return profile;
        }
    }
    // Perfil genérico M708 é o padrão seguro de fallback em mesas UGEE.
    &MODEL_PROFILES[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_longest_profile_match_for_e12plus() {
        let e12plus = resolve_model_profile(0x0096, Some("UGEE E12PLUS"), Some("FW_E12PLUS"));
        assert_eq!(e12plus.name, "E12PLUS");
        assert_eq!(e12plus.ring_num, 1);

        let e12 = resolve_model_profile(0x0096, Some("UGEE E12"), None);
        assert_eq!(e12.name, "E12");
        assert_eq!(e12.ring_num, 0);
    }
}
