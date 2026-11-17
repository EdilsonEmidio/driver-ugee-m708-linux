use crate::error::{DriverError, DriverResult};
use std::fs::OpenOptions;
use std::os::fd::AsRawFd;
use std::path::Path;

/// Report ID proprietário das interfaces UGEE (`0xFF0A`).
pub const UGEE_REPORT_ID: u8 = 0x02;
/// Usage Page proprietária usada pela interface vendor digitizer.
pub const UGEE_VENDOR_USAGE_PAGE: u16 = 0xFF0A;
/// Usage esperado na coleção top-level da interface vendor.
pub const UGEE_VENDOR_USAGE: u16 = 0x0001;
/// Resolução padrão (LPI) conforme fallback do driver original (`0x13D8`).
pub const DEFAULT_LPI: u16 = 5080;

/// Modo de handshake determinado pelo descritor USB `0x6E` (`payload[1] >= 0x10`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeMode {
    Mode02 = 0x02,
    Mode04 = 0x04,
}

impl HandshakeMode {
    pub fn as_byte(self) -> u8 {
        self as u8
    }
}

/// Informações extraídas do HID Report Descriptor de uma interface `hidraw`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportDescriptorInfo {
    pub usage_page: u16,
    pub usage: u16,
    pub report_id: Option<u8>,
    pub input_report_bytes: usize,
    pub output_report_bytes: usize,
    pub feature_report_bytes: usize,
}

impl ReportDescriptorInfo {
    /// Retorna `true` se esta interface for a interface vendor raw UGEE (`0xFF0A` / `0x0001`).
    pub fn is_ugee_vendor_interface(&self) -> bool {
        self.usage_page == UGEE_VENDOR_USAGE_PAGE && self.usage == UGEE_VENDOR_USAGE
    }
}

/// Faz o parsing de um HID Report Descriptor bruto para extrair Usage Page, Usage e tamanhos de relatório.
pub fn parse_report_descriptor(bytes: &[u8]) -> DriverResult<ReportDescriptorInfo> {
    if bytes.is_empty() {
        return Err(DriverError::InvalidPacket(
            "report descriptor vazio".to_string(),
        ));
    }

    let mut i = 0usize;
    let mut current_usage_page: u16 = 0;
    let mut top_usage_page: Option<u16> = None;
    let mut top_usage: Option<u16> = None;
    let mut current_usages: Vec<u16> = Vec::new();
    let mut report_size: usize = 0;
    let mut report_count: usize = 0;
    let mut current_report_id: Option<u8> = None;
    let mut input_bits: usize = 0;
    let mut output_bits: usize = 0;
    let mut feature_bits: usize = 0;
    let mut matched_report_id: Option<u8> = None;

    while i < bytes.len() {
        let prefix = bytes[i];
        if prefix == 0xFE {
            // Long item: 0xFE, data_size, long_item_tag, data...
            if i + 2 >= bytes.len() {
                break;
            }
            let data_len = bytes[i + 1] as usize;
            i += 3 + data_len;
            continue;
        }

        let size_code = prefix & 0x03;
        let data_len = match size_code {
            0 => 0,
            1 => 1,
            2 => 2,
            _ => 4,
        };
        if i + 1 + data_len > bytes.len() {
            return Err(DriverError::InvalidPacket(
                "item truncado no report descriptor HID".to_string(),
            ));
        }

        let item_type = (prefix >> 2) & 0x03;
        let tag = (prefix >> 4) & 0x0F;
        let data = &bytes[i + 1..i + 1 + data_len];
        let val = read_hid_uint(data);

        match (item_type, tag) {
            // Global: Usage Page (0x00)
            (1, 0) => {
                current_usage_page = val as u16;
                if top_usage_page.is_none() {
                    top_usage_page = Some(current_usage_page);
                }
            }
            // Global: Report Size (0x07)
            (1, 7) => report_size = val as usize,
            // Global: Report ID (0x08)
            (1, 8) => {
                let id = val as u8;
                current_report_id = Some(id);
                if current_usage_page == UGEE_VENDOR_USAGE_PAGE || matched_report_id.is_none() {
                    matched_report_id = Some(id);
                }
            }
            // Global: Report Count (0x09)
            (1, 9) => report_count = val as usize,
            // Local: Usage (0x00)
            (2, 0) => {
                current_usages.push(val as u16);
                if top_usage.is_none() {
                    top_usage = Some(val as u16);
                }
            }
            // Main: Input (0x08), Output (0x09), Feature (0x0B)
            (0, 8) => {
                if current_report_id.is_none() || current_report_id == Some(UGEE_REPORT_ID) {
                    input_bits =
                        input_bits.saturating_add(report_size.saturating_mul(report_count));
                }
                current_usages.clear();
            }
            (0, 9) => {
                if current_report_id.is_none() || current_report_id == Some(UGEE_REPORT_ID) {
                    output_bits =
                        output_bits.saturating_add(report_size.saturating_mul(report_count));
                }
                current_usages.clear();
            }
            (0, 11) => {
                if current_report_id.is_none() || current_report_id == Some(UGEE_REPORT_ID) {
                    feature_bits =
                        feature_bits.saturating_add(report_size.saturating_mul(report_count));
                }
                current_usages.clear();
            }
            // Main: Collection (0x0A)
            (0, 10) => {
                current_usages.clear();
            }
            _ => {}
        }

        i += 1 + data_len;
    }

    let id_overhead = usize::from(matched_report_id.is_some());
    let input_report_bytes =
        bits_to_bytes(input_bits) + if input_bits > 0 { id_overhead } else { 0 };
    let output_report_bytes =
        bits_to_bytes(output_bits) + if output_bits > 0 { id_overhead } else { 0 };
    let feature_report_bytes =
        bits_to_bytes(feature_bits) + if feature_bits > 0 { id_overhead } else { 0 };

    Ok(ReportDescriptorInfo {
        usage_page: top_usage_page.unwrap_or(0),
        usage: top_usage.unwrap_or(0),
        report_id: matched_report_id,
        input_report_bytes,
        output_report_bytes,
        feature_report_bytes,
    })
}

fn read_hid_uint(data: &[u8]) -> u32 {
    let mut acc = 0u32;
    for (idx, &byte) in data.iter().enumerate() {
        acc |= u32::from(byte) << (idx * 8);
    }
    acc
}

fn bits_to_bytes(bits: usize) -> usize {
    bits.div_ceil(8)
}

/// Capacidades físicas decodificadas do String Descriptor proprietário `0x64` (`0x403d40`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor64 {
    pub max_x: u32,
    pub max_y: u32,
    pub express_keys: u8,
    pub max_pressure: u16,
    pub resolution_lpi: u16,
    pub width_hundredths_inch: u32,
    pub height_hundredths_inch: u32,
    pub raw_payload: Vec<u8>,
}

/// Capacidades/flags decodificadas do String Descriptor proprietário `0x6E`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor6E {
    pub handshake_mode: HandshakeMode,
    pub proto_byte: u8,
    pub raw_payload: Vec<u8>,
}

/// Conjunto completo de descritores USB lidos da mesa UGEE.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UsbDeviceDescriptors {
    pub manufacturer: Option<String>,
    pub product_name: Option<String>,
    pub serial_number: Option<String>,
    pub ic_version: Option<String>,
    pub ic_data: Option<String>,
    pub physical: Option<Descriptor64>,
    pub capabilities: Option<Descriptor6E>,
    pub descriptor_83: Option<Vec<u8>>,
}

/// Remove os 2 bytes de cabeçalho USB (`[bLength, 0x03]`) caso estejam presentes.
pub fn strip_usb_string_header(raw: &[u8]) -> &[u8] {
    if raw.len() >= 2 && raw[1] == 0x03 {
        let declared_len = usize::from(raw[0]);
        if declared_len >= 2 && declared_len <= raw.len() {
            &raw[2..declared_len]
        } else {
            &raw[2..]
        }
    } else {
        raw
    }
}

/// Decodifica o descritor binário `0x64` seguindo exatamente o disassembly `0x403d93..0x403e13`.
pub fn parse_descriptor_64(raw: &[u8]) -> DriverResult<Descriptor64> {
    let b = strip_usb_string_header(raw);
    if b.len() < 10 {
        return Err(DriverError::InvalidDescriptor {
            index: 0x64,
            reason: format!("payload curto demais ({} bytes, esperado >= 10)", b.len()),
        });
    }

    let x_hi = if b.len() > 10 { u32::from(b[10]) } else { 0 };
    let y_hi = if b.len() > 11 { u32::from(b[11]) } else { 0 };

    let max_x = (x_hi << 16) | (u32::from(b[1]) << 8) | u32::from(b[0]);
    let max_y = (y_hi << 16) | (u32::from(b[3]) << 8) | u32::from(b[2]);
    let express_keys = b[4];
    let max_pressure = u16::from_le_bytes([b[6], b[7]]);
    let raw_lpi = u16::from_le_bytes([b[8], b[9]]);
    let resolution_lpi = if raw_lpi == 0 { DEFAULT_LPI } else { raw_lpi };

    let width_hundredths_inch = (max_x * 100) / u32::from(resolution_lpi);
    let height_hundredths_inch = (max_y * 100) / u32::from(resolution_lpi);

    Ok(Descriptor64 {
        max_x,
        max_y,
        express_keys,
        max_pressure,
        resolution_lpi,
        width_hundredths_inch,
        height_hundredths_inch,
        raw_payload: b.to_vec(),
    })
}

/// Decodifica o descritor binário `0x6E` (`payload[1] >= 0x10` -> `HandshakeMode::Mode04`).
pub fn parse_descriptor_6e(raw: &[u8]) -> DriverResult<Descriptor6E> {
    let b = strip_usb_string_header(raw);
    if b.len() < 2 {
        return Err(DriverError::InvalidDescriptor {
            index: 0x6E,
            reason: format!("payload curto demais ({} bytes, esperado >= 2)", b.len()),
        });
    }
    let proto_byte = b[1];
    let handshake_mode = if proto_byte >= 0x10 {
        HandshakeMode::Mode04
    } else {
        HandshakeMode::Mode02
    };
    Ok(Descriptor6E {
        handshake_mode,
        proto_byte,
        raw_payload: b.to_vec(),
    })
}

/// Decodifica uma USB String Descriptor UTF-16LE padrão (`0x01..0x05`), removendo `\0` finais.
pub fn decode_utf16le_string_descriptor(raw: &[u8]) -> Option<String> {
    let payload = strip_usb_string_header(raw);
    if payload.len() < 2 {
        return None;
    }
    let (chunks, _) = payload.as_chunks::<2>();
    let units: Vec<u16> = chunks
        .iter()
        .map(|chunk| u16::from_le_bytes(*chunk))
        .take_while(|&code| code != 0)
        .collect();
    let decoded = String::from_utf16_lossy(&units);
    let trimmed = decoded.trim_end_matches('\0').trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[repr(C)]
struct UsbdevfsCtrltransfer {
    b_request_type: u8,
    b_request: u8,
    w_value: u16,
    w_index: u16,
    w_length: u16,
    timeout: u32,
    data: *mut libc::c_void,
}

const fn ioc(dir: u32, kind: u8, nr: u8, size: usize) -> libc::Ioctl {
    let val = (dir << 30) | ((size as u32) << 16) | ((kind as u32) << 8) | (nr as u32);
    val as libc::Ioctl
}

const IOC_READ: u32 = 2;
const IOC_WRITE: u32 = 1;
const USBDEVFS_CONTROL: libc::Ioctl = ioc(
    IOC_READ | IOC_WRITE,
    b'U',
    0,
    std::mem::size_of::<UsbdevfsCtrltransfer>(),
);

fn read_usb_string_descriptor_on_fd(
    file: &std::fs::File,
    usb_dev_path: &Path,
    index: u8,
) -> DriverResult<Vec<u8>> {
    let mut buf = vec![0u8; 255];
    let lang_id: u16 = if index == 0 { 0x0000 } else { 0x0409 };
    let mut ctrl = UsbdevfsCtrltransfer {
        b_request_type: 0x80, // Device-to-Host | Standard | Device
        b_request: 0x06,      // GET_DESCRIPTOR
        w_value: (0x03u16 << 8) | u16::from(index),
        w_index: lang_id,
        w_length: buf.len() as u16,
        timeout: 500,
        data: buf.as_mut_ptr().cast::<libc::c_void>(),
    };

    // SAFETY: `file` é um descritor válido aberto para `/dev/bus/usb/...`, `ctrl` aponta
    // para `buf` de 255 bytes válido e alinhado pela duração da chamada síncrona `ioctl`.
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), USBDEVFS_CONTROL, &mut ctrl) };
    if rc < 0 {
        return Err(DriverError::Ioctl {
            op: "USBDEVFS_CONTROL",
            path: usb_dev_path.to_path_buf(),
            source: std::io::Error::last_os_error(),
        });
    }

    let len = (rc as usize).min(buf.len());
    buf.truncate(len);
    Ok(buf)
}

/// Lê um USB String Descriptor bruto via `/dev/bus/usb/BBB/DDD` usando `USBDEVFS_CONTROL`.
pub fn read_usb_string_descriptor_raw(usb_dev_path: &Path, index: u8) -> DriverResult<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(usb_dev_path)
        .map_err(|source| DriverError::Io {
            path: usb_dev_path.to_path_buf(),
            source,
        })?;
    read_usb_string_descriptor_on_fd(&file, usb_dev_path, index)
}

/// Consulta todos os descritores USB relevantes (`0x01..0x05`, `0x64`, `0x6E`, `0x83`).
///
/// Retorna erro caso o nó `usbfs` não possa ser aberto ou caso nenhum descritor possa ser
/// obtido via `USBDEVFS_CONTROL`, permitindo o fallback para strings via `sysfs`.
pub fn read_all_usb_descriptors(usb_dev_path: &Path) -> DriverResult<UsbDeviceDescriptors> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(usb_dev_path)
        .map_err(|source| DriverError::Io {
            path: usb_dev_path.to_path_buf(),
            source,
        })?;

    let mut success_count = 0usize;
    let mut last_err: Option<DriverError> = None;

    let mut fetch = |idx: u8| -> Option<Vec<u8>> {
        match read_usb_string_descriptor_on_fd(&file, usb_dev_path, idx) {
            Ok(raw) => {
                success_count += 1;
                Some(raw)
            }
            Err(err) => {
                last_err = Some(err);
                None
            }
        }
    };

    let mut out = UsbDeviceDescriptors {
        manufacturer: fetch(0x01).and_then(|raw| decode_utf16le_string_descriptor(&raw)),
        product_name: fetch(0x02).and_then(|raw| decode_utf16le_string_descriptor(&raw)),
        serial_number: fetch(0x03).and_then(|raw| decode_utf16le_string_descriptor(&raw)),
        ic_version: fetch(0x04).and_then(|raw| decode_utf16le_string_descriptor(&raw)),
        ic_data: fetch(0x05).and_then(|raw| decode_utf16le_string_descriptor(&raw)),
        ..UsbDeviceDescriptors::default()
    };

    if let Some(raw_64) = fetch(0x64) {
        out.physical = parse_descriptor_64(&raw_64).ok();
    }
    if let Some(raw_6e) = fetch(0x6E) {
        out.capabilities = parse_descriptor_6e(&raw_6e).ok();
    }
    if let Some(raw_83) = fetch(0x83) {
        out.descriptor_83 = Some(strip_usb_string_header(&raw_83).to_vec());
    }

    if success_count == 0 {
        if let Some(err) = last_err {
            return Err(err);
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_m708_report_descriptor() -> Result<(), Box<dyn std::error::Error>> {
        // Report descriptor real capturado de /dev/hidraw7 (UGTABLET M708 28bd:0924)
        let bytes = [
            0x06, 0x0A, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x02, 0x09, 0x02, 0x15, 0x00, 0x26,
            0xFF, 0x00, 0x75, 0x08, 0x95, 0x09, 0x81, 0x02, 0x09, 0x03, 0x15, 0x00, 0x26, 0xFF,
            0x00, 0x75, 0x08, 0x95, 0x09, 0x91, 0x02, 0xC0,
        ];
        let info = parse_report_descriptor(&bytes)?;
        assert_eq!(info.usage_page, 0xFF0A);
        assert_eq!(info.usage, 0x0001);
        assert_eq!(info.report_id, Some(0x02));
        assert_eq!(info.input_report_bytes, 10);
        assert_eq!(info.output_report_bytes, 10);
        assert!(info.is_ugee_vendor_interface());
        Ok(())
    }

    #[test]
    fn parses_real_m708_descriptor_64() -> Result<(), Box<dyn std::error::Error>> {
        // Bytes reais do índice 0x64 de UGTABLET M708 (28bd:0924)
        let raw_64 = [
            0x0E, 0x03, 0x6A, 0xC6, 0x0A, 0x77, 0x08, 0x00, 0xFF, 0x1F, 0xD8, 0x13, 0x00, 0x00,
        ];
        let desc = parse_descriptor_64(&raw_64)?;
        assert_eq!(desc.max_x, 50794);
        assert_eq!(desc.max_y, 30474);
        assert_eq!(desc.express_keys, 8);
        assert_eq!(desc.max_pressure, 8191);
        assert_eq!(desc.resolution_lpi, 5080);
        assert_eq!(desc.width_hundredths_inch, 999);
        assert_eq!(desc.height_hundredths_inch, 599);
        Ok(())
    }

    #[test]
    fn parses_24bit_descriptor_64_without_usb_header() -> Result<(), Box<dyn std::error::Error>> {
        // Payload de 12 bytes representando eixo 24 bits (ex: telas grandes)
        let payload = [
            0x00, 0x00, 0x00, 0x00, 0x0A, 0x00, 0xFF, 0x3F, 0x00, 0x00, 0x01, 0x02,
        ];
        let desc = parse_descriptor_64(&payload)?;
        assert_eq!(desc.max_x, 0x01_0000);
        assert_eq!(desc.max_y, 0x02_0000);
        assert_eq!(desc.max_pressure, 16383);
        assert_eq!(desc.resolution_lpi, 5080); // fallback de lpi == 0
        Ok(())
    }

    #[test]
    fn parses_real_m708_descriptor_6e() -> Result<(), Box<dyn std::error::Error>> {
        // Bytes reais do índice 0x6E de UGTABLET M708 (28bd:0924)
        let raw_6e = [
            0x0B, 0x03, 0x00, 0x10, 0x00, 0x01, 0x08, 0x00, 0x00, 0x00, 0x03,
        ];
        let desc = parse_descriptor_6e(&raw_6e)?;
        assert_eq!(desc.proto_byte, 0x10);
        assert_eq!(desc.handshake_mode, HandshakeMode::Mode04);

        let raw_legacy = [0x04, 0x03, 0x00, 0x08];
        let legacy = parse_descriptor_6e(&raw_legacy)?;
        assert_eq!(legacy.handshake_mode, HandshakeMode::Mode02);
        Ok(())
    }

    #[test]
    fn decodes_utf16le_strings() {
        let mut raw_ic = vec![0x24u8, 0x03];
        for unit in "UG901_BPU1006_V11".encode_utf16() {
            raw_ic.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(
            decode_utf16le_string_descriptor(&raw_ic).as_deref(),
            Some("UG901_BPU1006_V11")
        );
    }

    #[test]
    fn malformed_usb_header_and_huge_report_count_do_not_panic() {
        assert_eq!(strip_usb_string_header(&[0x00, 0x03, 0x00]), &[0x00]);
        assert!(strip_usb_string_header(&[0x01, 0x03]).is_empty());
        assert!(parse_descriptor_64(&[0x00, 0x03, 0x00]).is_err());

        // ReportSize=0xFFFFFFFF, ReportCount=0xFFFFFFFF -> saturating_mul/add evita panic
        let malformed_rdesc = [
            0x77, 0xFF, 0xFF, 0xFF, 0xFF, 0x97, 0xFF, 0xFF, 0xFF, 0xFF, 0x81, 0x02,
        ];
        assert!(parse_report_descriptor(&malformed_rdesc).is_ok());
    }
}
