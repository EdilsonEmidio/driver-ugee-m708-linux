use crate::error::{DriverError, DriverResult};
use crate::hid::models::pid_quirks;

/// Estado decodificado de um pacote da caneta (`CTablet::ParseData` em `0x410637..0x41088b`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PenEvent {
    pub in_range: bool,
    pub tip_switch: bool,
    pub barrel_1: bool,
    pub barrel_2: bool,
    pub eraser: bool,
    pub relative_hint: bool,
    pub x: u32,
    pub y: u32,
    pub pressure: u16,
    pub tilt_x: i8,
    pub tilt_y: i8,
    pub raw_status: u8,
}

/// Evento de Express Keys e/ou Touch Ring / Dial (`0x410493..0x410636`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpressKeyEvent {
    /// Bitmask com até 40 teclas (`bit 0` = K1, ..., `bit 39` = K40).
    pub keys_mask: u64,
    /// Índice do anel/dial (`0` = Ring 1, `1` = Ring 2) caso haja giro nesta leitura.
    pub ring_index: u8,
    /// Direção do giro (`+1` horário CW, `-1` anti-horário CCW, `0` sem movimento).
    pub ring_delta: i8,
}

/// Eventos de sistema pré-filtrados antes de `CTablet::ParseData` (`0x411970..0x411b9b`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemEvent {
    LedBrightness { target: u8, value: u8 },
    Battery { raw_level: u8 },
    Wake,
}

/// Pacote HID decodificado vindo da interface `0xFF0A` (ou report `0x06`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabletPacket {
    Pen(PenEvent),
    ExpressKey(ExpressKeyEvent),
    OutOfRange { raw_status: u8 },
    System(SystemEvent),
    Unknown { report_id: u8, cmd: u8 },
}

/// Mantém estado inter-pacotes necessário para decodificação em quadratura do Touch Ring.
#[derive(Debug, Default)]
pub struct PacketParserState {
    prev_dial: Option<u8>,
}

impl PacketParserState {
    /// Cria um novo estado limpo de parser.
    pub fn new() -> Self {
        Self::default()
    }

    /// Decodifica um pacote bruto seguindo `CTablet::ParseData` (`0x410440`) e pré-filtros (`0x411970`).
    pub fn parse(&mut self, pid: u16, buf: &[u8]) -> DriverResult<TabletPacket> {
        if buf.len() < 8 {
            return Err(DriverError::InvalidPacket(format!(
                "tamanho insuficiente: {} bytes (mínimo 8)",
                buf.len()
            )));
        }

        let quirks = pid_quirks(pid);

        // Relatório alternativo de Express Keys (Report ID 0x06, 4 bytes de bitmask em buf[1..=4])
        if buf[0] == 0x06 {
            let mut mask = 0u64;
            for (byte_idx, &byte) in buf[1..=4].iter().enumerate() {
                mask |= u64::from(byte) << (byte_idx * 8);
            }
            return Ok(TabletPacket::ExpressKey(ExpressKeyEvent {
                keys_mask: mask,
                ring_index: 0,
                ring_delta: 0,
            }));
        }

        let cmd = buf[1];

        // Pré-filtros de sistema (0x411970..0x411b9b)
        if cmd == 0xB4 && buf[2] == 0x01 && matches!(buf[3], 0x00 | 0x01) {
            return Ok(TabletPacket::System(SystemEvent::LedBrightness {
                target: buf[4],
                value: buf[5],
            }));
        }
        if cmd == 0xF2 {
            return Ok(TabletPacket::System(SystemEvent::Battery {
                raw_level: buf[3],
            }));
        }
        if cmd == 0xF8 && buf[2] == 0x02 && buf[3] == 0x01 {
            return Ok(TabletPacket::System(SystemEvent::Wake));
        }

        // Pacotes de Express Keys e Touch Ring (`buf[1] == 0xF0`)
        if cmd == 0xF0 {
            let mut mask = 0u64;
            for (byte_idx, &byte) in buf[2..=6].iter().enumerate() {
                mask |= u64::from(byte) << (byte_idx * 8);
            }
            let (ring_index, ring_delta) = self.decode_dial(buf[7], quirks.invert_ring_quadrature);
            return Ok(TabletPacket::ExpressKey(ExpressKeyEvent {
                keys_mask: mask,
                ring_index,
                ring_delta,
            }));
        }

        // Pacotes de Caneta: `(buf[1] & 0xF0) == 0xA0` ou `buf[1] == 0xC0`
        if (cmd & 0xF0) == 0xA0 || cmd == 0xC0 {
            let mut x = u32::from(u16::from_le_bytes([buf[2], buf[3]]));
            let mut y = u32::from(u16::from_le_bytes([buf[4], buf[5]]));
            if buf.len() == 12 {
                x |= u32::from(buf[10]) << 16;
                y |= u32::from(buf[11]) << 16;
            }
            let tip_switch = (cmd & 0x01) != 0;
            let raw_pressure = u16::from_le_bytes([buf[6], buf[7]]);
            let pressure = if tip_switch { raw_pressure } else { 0 };
            let (tilt_x, tilt_y) = if buf.len() >= 10 {
                (buf[8] as i8, buf[9] as i8)
            } else {
                (0, 0)
            };

            return Ok(TabletPacket::Pen(PenEvent {
                in_range: true,
                tip_switch,
                barrel_1: (cmd & 0x02) != 0,
                barrel_2: (cmd & 0x04) != 0,
                eraser: (cmd & 0x08) != 0,
                relative_hint: cmd == 0xC0,
                x,
                y,
                pressure,
                tilt_x,
                tilt_y,
                raw_status: cmd,
            }));
        }

        // Exceção legada PID 0x0201 (`0x410645`, helper `0x4101e0`) — avaliada após `0xF0` e caneta
        if quirks.legacy_1byte_express_keys && cmd != 0x01 {
            let mut mask = 0u64;
            let end = buf.len().min(23);
            for (idx, &val) in buf[3..end].iter().enumerate() {
                if val != 0 && idx < 40 && idx != 9 && idx != 10 {
                    mask |= 1u64 << idx;
                }
            }
            let ring_delta = if buf.get(12).copied().unwrap_or(0) != 0 {
                1
            } else if buf.get(13).copied().unwrap_or(0) != 0 {
                -1
            } else {
                0
            };
            // `cmd == 0x80` sem teclas nem giro ativo sinaliza Out-of-Range normalmente.
            if cmd != 0x80 || mask != 0 || ring_delta != 0 {
                return Ok(TabletPacket::ExpressKey(ExpressKeyEvent {
                    keys_mask: mask,
                    ring_index: 0,
                    ring_delta,
                }));
            }
        }

        // Qualquer outro status no pacote da caneta sinaliza Out-of-Range (`flags = 0x80` em 0x41069e)
        if buf[0] == 0x02 && matches!(cmd, 0x00 | 0x80 | 0xC1..=0xEF) {
            return Ok(TabletPacket::OutOfRange { raw_status: cmd });
        }

        Ok(TabletPacket::Unknown {
            report_id: buf[0],
            cmd,
        })
    }

    fn decode_dial(&mut self, byte7: u8, invert_quadrature: bool) -> (u8, i8) {
        // Modo Discreto (`0x410599..0x410632`)
        match byte7 {
            0x01 => return (0, 1),
            0x02 | 0xFF => return (0, -1),
            0x04 | 0x10 => return (1, 1),
            0x08 | 0x20 => return (1, -1),
            _ => {}
        }

        // Modo Quadratura (`0x00 -> 0x40 -> 0xC0 -> 0x80 -> 0x00`, `0x4104cf..0x410593`)
        if matches!(byte7, 0x00 | 0x40 | 0x80 | 0xC0) {
            let mut delta: i8 = match (self.prev_dial, byte7) {
                (Some(0x00), 0x40)
                | (Some(0x40), 0xC0)
                | (Some(0xC0), 0x80)
                | (Some(0x80), 0x00) => 1,
                (Some(0x00), 0x80)
                | (Some(0x80), 0xC0)
                | (Some(0xC0), 0x40)
                | (Some(0x40), 0x00) => -1,
                _ => 0,
            };
            self.prev_dial = Some(byte7);
            if invert_quadrature {
                delta = -delta;
            }
            return (0, delta);
        }

        (0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PID: u16 = 0x0924;

    #[test]
    fn parses_stylus_hover_10_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = PacketParserState::new();
        let raw = [0x02, 0xA0, 0x10, 0x27, 0x88, 0x13, 0x00, 0x00, 0xFC, 0x05];
        let pkt = state.parse(TEST_PID, &raw)?;
        assert_eq!(
            pkt,
            TabletPacket::Pen(PenEvent {
                in_range: true,
                tip_switch: false,
                barrel_1: false,
                barrel_2: false,
                eraser: false,
                relative_hint: false,
                x: 10000,
                y: 5000,
                pressure: 0,
                tilt_x: -4,
                tilt_y: 5,
                raw_status: 0xA0,
            })
        );
        Ok(())
    }

    #[test]
    fn parses_stylus_tip_btn1_and_tilt_10_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = PacketParserState::new();
        let raw = [0x02, 0xA3, 0xFF, 0x7F, 0x00, 0x40, 0xFF, 0x1F, 0xE2, 0x1E];
        let pkt = state.parse(TEST_PID, &raw)?;
        assert_eq!(
            pkt,
            TabletPacket::Pen(PenEvent {
                in_range: true,
                tip_switch: true,
                barrel_1: true,
                barrel_2: false,
                eraser: false,
                relative_hint: false,
                x: 32767,
                y: 16384,
                pressure: 8191,
                tilt_x: -30,
                tilt_y: 30,
                raw_status: 0xA3,
            })
        );
        Ok(())
    }

    #[test]
    fn parses_stylus_24bit_12_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = PacketParserState::new();
        let raw = [
            0x02, 0xA1, 0x40, 0x9C, 0x20, 0x4E, 0xFF, 0x3F, 0x00, 0x00, 0x01, 0x01,
        ];
        let pkt = state.parse(TEST_PID, &raw)?;
        assert_eq!(
            pkt,
            TabletPacket::Pen(PenEvent {
                in_range: true,
                tip_switch: true,
                barrel_1: false,
                barrel_2: false,
                eraser: false,
                relative_hint: false,
                x: 0x019C40,
                y: 0x014E20,
                pressure: 16383,
                tilt_x: 0,
                tilt_y: 0,
                raw_status: 0xA1,
            })
        );
        Ok(())
    }

    #[test]
    fn parses_eraser_and_barrel2_bits() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = PacketParserState::new();
        let raw_b2 = [0x02, 0xA4, 0x01, 0x00, 0x02, 0x00, 0x10, 0x00, 0x00, 0x00];
        let pkt_b2 = state.parse(TEST_PID, &raw_b2)?;
        if let TabletPacket::Pen(pen) = pkt_b2 {
            assert!(pen.barrel_2);
            assert!(!pen.tip_switch);
            assert_eq!(pen.pressure, 0);
        } else {
            return Err("esperava pacote Pen para 0xA4".into());
        }

        let raw_eraser = [0x02, 0xA9, 0x01, 0x00, 0x02, 0x00, 0x64, 0x00, 0x00, 0x00];
        let pkt_er = state.parse(TEST_PID, &raw_eraser)?;
        if let TabletPacket::Pen(pen) = pkt_er {
            assert!(pen.eraser);
            assert!(pen.tip_switch);
            assert_eq!(pen.pressure, 100);
        } else {
            return Err("esperava pacote Pen para 0xA9".into());
        }
        Ok(())
    }

    #[test]
    fn parses_stylus_out_of_range() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = PacketParserState::new();
        let raw = [0x02, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let pkt = state.parse(TEST_PID, &raw)?;
        assert_eq!(pkt, TabletPacket::OutOfRange { raw_status: 0x80 });
        Ok(())
    }

    #[test]
    fn parses_express_keys_f0_and_report_06() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = PacketParserState::new();
        // K1 e K10 pressionadas
        let raw_f0 = [0x02, 0xF0, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let pkt_f0 = state.parse(TEST_PID, &raw_f0)?;
        assert_eq!(
            pkt_f0,
            TabletPacket::ExpressKey(ExpressKeyEvent {
                keys_mask: (1 << 0) | (1 << 9),
                ring_index: 0,
                ring_delta: 0,
            })
        );

        // K3 via report 0x06
        let raw_06 = [0x06, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let pkt_06 = state.parse(TEST_PID, &raw_06)?;
        assert_eq!(
            pkt_06,
            TabletPacket::ExpressKey(ExpressKeyEvent {
                keys_mask: 1 << 2,
                ring_index: 0,
                ring_delta: 0,
            })
        );
        Ok(())
    }

    #[test]
    fn parses_touch_rings_discrete_and_quadrature() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = PacketParserState::new();

        let cw1 = [0x02, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00];
        let ccw1 = [0x02, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00];
        let cw2 = [0x02, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00];
        let ccw2 = [0x02, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00];

        assert_eq!(
            state.parse(TEST_PID, &cw1)?,
            TabletPacket::ExpressKey(ExpressKeyEvent {
                keys_mask: 0,
                ring_index: 0,
                ring_delta: 1,
            })
        );
        assert_eq!(
            state.parse(TEST_PID, &ccw1)?,
            TabletPacket::ExpressKey(ExpressKeyEvent {
                keys_mask: 0,
                ring_index: 0,
                ring_delta: -1,
            })
        );
        assert_eq!(
            state.parse(TEST_PID, &cw2)?,
            TabletPacket::ExpressKey(ExpressKeyEvent {
                keys_mask: 0,
                ring_index: 1,
                ring_delta: 1,
            })
        );
        assert_eq!(
            state.parse(TEST_PID, &ccw2)?,
            TabletPacket::ExpressKey(ExpressKeyEvent {
                keys_mask: 0,
                ring_index: 1,
                ring_delta: -1,
            })
        );

        // Quadratura ciclo completo CW (0x00 -> 0x40 -> 0xC0 -> 0x80 -> 0x00) e CCW
        let mut q_state = PacketParserState::new();
        for &(val, expected) in &[
            (0x00u8, 0i8),
            (0x40, 1),
            (0xC0, 1),
            (0x80, 1),
            (0x00, 1),
            (0x80, -1),
            (0xC0, -1),
            (0x40, -1),
            (0x00, -1),
        ] {
            let pkt = [0x02, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, val, 0x00, 0x00];
            if let TabletPacket::ExpressKey(ev) = q_state.parse(TEST_PID, &pkt)? {
                assert_eq!(
                    ev.ring_delta, expected,
                    "quadratura falhou no valor {val:#04x}"
                );
            } else {
                return Err("esperava ExpressKey na quadratura".into());
            }
        }

        // Quadratura com inversão para PID 0x0074
        let q1 = [0x02, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00];
        let q2 = [0x02, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00];
        let _ = state.parse(0x0074, &q1)?;
        assert_eq!(
            state.parse(0x0074, &q2)?,
            TabletPacket::ExpressKey(ExpressKeyEvent {
                keys_mask: 0,
                ring_index: 0,
                ring_delta: -1,
            })
        );
        Ok(())
    }

    #[test]
    fn parses_legacy_pid_0201_pen_express_keys_and_ring() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut state = PacketParserState::new();
        // Caneta deve continuar funcionando mesmo em PID 0x0201
        let pen_raw = [0x02, 0xA1, 0x10, 0x27, 0x88, 0x13, 0x00, 0x08, 0x00, 0x00];
        assert!(matches!(
            state.parse(0x0201, &pen_raw)?,
            TabletPacket::Pen(_)
        ));

        // Out-of-range continua funcionando quando sem teclas ativas
        let oor_raw = [0x02, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(
            state.parse(0x0201, &oor_raw)?,
            TabletPacket::OutOfRange { raw_status: 0x80 }
        );

        // Tecla K1 (buf[3]) + Dial CW (buf[12]) sem sujar bit 9 da keys_mask
        let mut key_and_ring = [0u8; 20];
        key_and_ring[0] = 0x02;
        key_and_ring[1] = 0x02;
        key_and_ring[3] = 0x01; // K1 (idx=0)
        key_and_ring[12] = 0x01; // Ring CW (idx=9)
        assert_eq!(
            state.parse(0x0201, &key_and_ring)?,
            TabletPacket::ExpressKey(ExpressKeyEvent {
                keys_mask: 1 << 0,
                ring_index: 0,
                ring_delta: 1,
            })
        );
        Ok(())
    }

    #[test]
    fn rejects_short_packet() {
        let mut state = PacketParserState::new();
        let err = state.parse(TEST_PID, &[0x02, 0xA0, 0x00]);
        assert!(err.is_err());
    }
}
