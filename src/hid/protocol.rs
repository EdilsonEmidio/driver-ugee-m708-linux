use crate::hid::descriptors::HandshakeMode;

/// Report ID proprietário usado nos comandos de saída para a interface `0xFF0A`.
pub const UGEE_REPORT_ID: u8 = 0x02;

/// Monta o Output Report de ativação do modo raw proprietário (`[0x02, 0xB0, mode, 0x00, ...]`).
pub fn build_handshake_report(mode: HandshakeMode, report_len: usize) -> Vec<u8> {
    let len = report_len.max(10);
    let mut buf = vec![0u8; len];
    buf[0] = UGEE_REPORT_ID;
    buf[1] = 0xB0;
    buf[2] = mode.as_byte();
    buf
}

/// Monta o Output Report para alternar o dispositivo para modo Android/Pass (`mode = 0x04`, 64 bytes).
pub fn build_android_pass_report(report_len: usize) -> Vec<u8> {
    let len = report_len.max(64);
    let mut buf = vec![0u8; len];
    buf[0] = UGEE_REPORT_ID;
    buf[1] = 0xB0;
    buf[2] = HandshakeMode::Mode04.as_byte();
    buf
}

/// Monta o Output Report de consulta de bateria (`cmd = 4`, `[0x02, 0xB8, 0x04, ...]`).
pub fn build_battery_poll_report(report_len: usize) -> Vec<u8> {
    let len = report_len.max(10);
    let mut buf = vec![0u8; len];
    buf[0] = UGEE_REPORT_ID;
    buf[1] = 0xB8;
    buf[2] = 0x04;
    buf
}

/// Monta o Output Report de controle de brilho do LED (`cmd = 0` ou `cmd = 1`).
pub fn build_led_brightness_report(val1: u8, val2: Option<u8>, report_len: usize) -> Vec<u8> {
    let len = report_len.max(12);
    let mut buf = vec![0u8; len];
    buf[0] = UGEE_REPORT_ID;
    buf[1] = 0xB4;
    buf[2] = 0x01;
    match val2 {
        Some(v2) => {
            buf[3] = 0x01;
            buf[4] = val1;
            buf[5] = v2;
        }
        None => {
            buf[3] = 0x00;
            buf[4] = val1;
        }
    }
    buf
}

/// Monta o Output Report de consulta de dongle/BLE (`[0x02, 0x80, 0x06, sub_id, ...]`).
pub fn build_dongle_query_report(sub_id: u8, report_len: usize) -> Vec<u8> {
    let len = report_len.max(10);
    let mut buf = vec![0u8; len];
    buf[0] = UGEE_REPORT_ID;
    buf[1] = 0x80;
    buf[2] = 0x06;
    buf[3] = sub_id;
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_expected_m708_handshake() {
        let pkt = build_handshake_report(HandshakeMode::Mode04, 10);
        assert_eq!(
            pkt,
            [0x02, 0xB0, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn builds_legacy_handshake_with_minimum_len() {
        let pkt = build_handshake_report(HandshakeMode::Mode02, 4);
        assert_eq!(pkt.len(), 10);
        assert_eq!(&pkt[..4], &[0x02, 0xB0, 0x02, 0x00]);
    }
}
