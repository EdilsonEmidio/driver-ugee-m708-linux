use crate::error::{DriverError, DriverResult};
use crate::hid::descriptors::{
    parse_report_descriptor, read_all_usb_descriptors, HandshakeMode, ReportDescriptorInfo,
    UsbDeviceDescriptors, DEFAULT_LPI,
};
use crate::hid::models::{lookup_pid, resolve_model_profile, ModelProfile, UGEE_VENDOR_ID};
use crate::hid::packet::{PacketParserState, TabletPacket};
use crate::hid::protocol::build_handshake_report;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Metadados consolidados de um dispositivo UGEE descoberto no Linux (`hidraw` + `usbfs`).
#[derive(Debug, Clone)]
pub struct DiscoveredTablet {
    pub hidraw_path: PathBuf,
    pub sysfs_device_path: PathBuf,
    pub usb_dev_path: Option<PathBuf>,
    pub vid: u16,
    pub pid: u16,
    pub hid_name: String,
    pub report_info: ReportDescriptorInfo,
    pub descriptors: UsbDeviceDescriptors,
    pub profile: &'static ModelProfile,
}

impl DiscoveredTablet {
    /// Largura máxima bruta nos eixos X e Y (prioriza Descriptor `0x64`, com fallback no perfil).
    pub fn physical_bounds(&self) -> (u32, u32, u16, u16) {
        if let Some(ref phys) = self.descriptors.physical {
            (
                phys.max_x.max(1),
                phys.max_y.max(1),
                phys.max_pressure.max(1),
                phys.resolution_lpi.max(1),
            )
        } else {
            let max_pressure = if self.pid == 0x0927 {
                16383
            } else {
                self.profile.default_max_pressure
            };
            (
                self.profile.default_max_x,
                self.profile.default_max_y,
                max_pressure,
                self.profile.default_lpi.max(DEFAULT_LPI),
            )
        }
    }

    /// Quantidade de botões expressos detectada (`0x64` ou fallback do perfil XML).
    pub fn express_key_count(&self) -> u8 {
        self.descriptors
            .physical
            .as_ref()
            .map(|p| p.express_keys)
            .filter(|&k| k > 0)
            .unwrap_or(self.profile.key_num)
    }

    /// Modo de handshake selecionado via Descriptor `0x6E` (`Mode04` por padrão moderno).
    pub fn handshake_mode(&self) -> HandshakeMode {
        self.descriptors
            .capabilities
            .as_ref()
            .map(|c| c.handshake_mode)
            .unwrap_or(HandshakeMode::Mode04)
    }

    /// Comprimento esperado dos Output Reports para esta interface (`10` na M708).
    pub fn output_report_len(&self) -> usize {
        self.report_info.output_report_bytes.max(10)
    }
}

/// Representa uma conexão ativa com a interface vendor raw (`/dev/hidrawX`) de uma mesa UGEE.
pub struct TabletDevice {
    info: DiscoveredTablet,
    file: File,
    parser_state: PacketParserState,
    read_buf: Vec<u8>,
}

impl TabletDevice {
    /// Procura todas as interfaces `/dev/hidraw*` pertencentes ao VID `0x28BD`.
    pub fn discover_all() -> DriverResult<Vec<DiscoveredTablet>> {
        let sys_hidraw = Path::new("/sys/class/hidraw");
        let entries = fs::read_dir(sys_hidraw).map_err(|source| DriverError::Io {
            path: sys_hidraw.to_path_buf(),
            source,
        })?;

        let mut results = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with("hidraw") {
                continue;
            }

            let dev_link = entry.path().join("device");
            let Ok(sysfs_device_path) = fs::canonicalize(&dev_link) else {
                continue;
            };

            let Some((vid, pid, hid_name)) = read_uevent_hid_ids(&sysfs_device_path) else {
                continue;
            };
            if vid != UGEE_VENDOR_ID {
                continue;
            }

            let rdesc_path = sysfs_device_path.join("report_descriptor");
            let Ok(rdesc_bytes) = fs::read(&rdesc_path) else {
                continue;
            };
            let Ok(report_info) = parse_report_descriptor(&rdesc_bytes) else {
                continue;
            };

            let usb_dev_path = find_usb_bus_device(&sysfs_device_path);
            let mut descriptors = match usb_dev_path.as_deref() {
                Some(path) => match read_all_usb_descriptors(path) {
                    Ok(desc) => desc,
                    Err(err) => {
                        debug!(
                            path = %path.display(),
                            error = %err,
                            "falha ao ler descritores USB via usbfs; usando fallback sysfs"
                        );
                        read_sysfs_usb_strings(&sysfs_device_path)
                    }
                },
                None => read_sysfs_usb_strings(&sysfs_device_path),
            };

            if descriptors.product_name.is_none() && !hid_name.is_empty() {
                descriptors.product_name = Some(hid_name.clone());
            }

            let profile = resolve_model_profile(
                pid,
                descriptors.product_name.as_deref(),
                descriptors.ic_version.as_deref(),
            );

            results.push(DiscoveredTablet {
                hidraw_path: PathBuf::from("/dev").join(&*name_str),
                sysfs_device_path,
                usb_dev_path,
                vid,
                pid,
                hid_name,
                report_info,
                descriptors,
                profile,
            });
        }

        // Prioriza a interface vendor raw `UsagePage=0xFF0A, Usage=0x0001`.
        results.sort_by_key(|d| {
            (
                !d.report_info.is_ugee_vendor_interface(),
                d.hidraw_path.clone(),
            )
        });

        Ok(results)
    }

    /// Descobre e abre a primeira interface vendor raw (`0xFF0A`/`0x0001`) UGEE disponível.
    pub fn open_first() -> DriverResult<Self> {
        let candidates = Self::discover_all()?;
        let vendor = candidates
            .into_iter()
            .find(|d| d.report_info.is_ugee_vendor_interface())
            .ok_or(DriverError::DeviceNotFound)?;

        Self::open(vendor)
    }

    /// Abre o nó `/dev/hidraw*` informado e inicializa o estado interno.
    pub fn open(info: DiscoveredTablet) -> DriverResult<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&info.hidraw_path)
            .map_err(|source| DriverError::Io {
                path: info.hidraw_path.clone(),
                source,
            })?;

        let buf_len = info.report_info.input_report_bytes.max(64);
        let known_name = lookup_pid(info.pid)
            .map(|k| k.name)
            .unwrap_or(info.profile.name);
        info!(
            hidraw = %info.hidraw_path.display(),
            vid = format_args!("{:#06x}", info.vid),
            pid = format_args!("{:#06x}", info.pid),
            model = known_name,
            profile = info.profile.name,
            "mesa UGEE aberta"
        );

        Ok(Self {
            info,
            file,
            parser_state: PacketParserState::new(),
            read_buf: vec![0u8; buf_len],
        })
    }

    /// Retorna os metadados e descritores do dispositivo aberto.
    pub fn info(&self) -> &DiscoveredTablet {
        &self.info
    }

    /// Envia o pacote Output Report de ativação do modo vendor raw (`[0x02, 0xB0, mode, ...]`).
    pub fn send_handshake(&mut self) -> DriverResult<Vec<u8>> {
        let mode = self.info.handshake_mode();
        let report_len = self.info.output_report_len();
        let report = build_handshake_report(mode, report_len);

        self.file
            .write_all(&report)
            .and_then(|()| self.file.flush())
            .map_err(|source| DriverError::Io {
                path: self.info.hidraw_path.clone(),
                source,
            })?;

        debug!(
            hidraw = %self.info.hidraw_path.display(),
            mode = format_args!("{:#04x}", mode.as_byte()),
            bytes = report.len(),
            "handshake Output Report enviado com sucesso"
        );
        Ok(report)
    }

    /// Envia um comando Output Report arbitrário para `/dev/hidrawX`.
    pub fn send_output_report(&mut self, report: &[u8]) -> DriverResult<()> {
        self.file
            .write_all(report)
            .and_then(|()| self.file.flush())
            .map_err(|source| DriverError::Io {
                path: self.info.hidraw_path.clone(),
                source,
            })
    }

    /// Aguarda até `timeout_ms` por um pacote HID na interface `hidraw` e retorna o pacote decodificado.
    pub fn read_packet(&mut self, timeout_ms: i32) -> DriverResult<Option<TabletPacket>> {
        let mut pfd = libc::pollfd {
            fd: self.file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };

        // SAFETY: `pfd` é uma struct válida alocada na stack com comprimento 1 para `libc::poll`.
        let ready = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
        if ready < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == ErrorKind::Interrupted {
                return Ok(None);
            }
            return Err(DriverError::Io {
                path: self.info.hidraw_path.clone(),
                source: err,
            });
        }
        if ready == 0 {
            return Ok(None);
        }
        if (pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL)) != 0 {
            return Err(DriverError::DeviceDisconnected);
        }

        match self.file.read(&mut self.read_buf) {
            Ok(0) => Err(DriverError::DeviceDisconnected),
            Ok(n) => {
                let slice = &self.read_buf[..n];
                match self.parser_state.parse(self.info.pid, slice) {
                    Ok(pkt) => Ok(Some(pkt)),
                    Err(err) => {
                        warn!(
                            len = n,
                            error = %err,
                            "pacote HID ignorado por erro de formato"
                        );
                        Ok(None)
                    }
                }
            }
            Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {
                Ok(None)
            }
            Err(err)
                if matches!(
                    err.raw_os_error(),
                    Some(libc::ENODEV | libc::ENXIO | libc::EIO | libc::ESHUTDOWN)
                ) =>
            {
                Err(DriverError::DeviceDisconnected)
            }
            Err(source) => Err(DriverError::Io {
                path: self.info.hidraw_path.clone(),
                source,
            }),
        }
    }
}

fn read_uevent_hid_ids(sysfs_device_path: &Path) -> Option<(u16, u16, String)> {
    let uevent = fs::read_to_string(sysfs_device_path.join("uevent")).ok()?;
    let mut vid_pid: Option<(u16, u16)> = None;
    let mut hid_name = String::new();

    for line in uevent.lines() {
        if let Some(rest) = line.strip_prefix("HID_ID=") {
            // Formato: BUS:VENDOR:PRODUCT (ex: 0003:000028BD:00000924)
            let mut parts = rest.split(':');
            let _bus = parts.next();
            let vid = parts.next().and_then(|s| u16::from_str_radix(s, 16).ok());
            let pid = parts.next().and_then(|s| u16::from_str_radix(s, 16).ok());
            if let (Some(v), Some(p)) = (vid, pid) {
                vid_pid = Some((v, p));
            }
        } else if let Some(rest) = line.strip_prefix("HID_NAME=") {
            hid_name = rest.trim().to_string();
        }
    }

    vid_pid.map(|(v, p)| (v, p, hid_name))
}

fn find_usb_bus_device(sysfs_hid_path: &Path) -> Option<PathBuf> {
    let mut current = Some(sysfs_hid_path);
    while let Some(dir) = current {
        let busnum_path = dir.join("busnum");
        let devnum_path = dir.join("devnum");
        if busnum_path.exists() && devnum_path.exists() {
            let busnum: u32 = fs::read_to_string(busnum_path).ok()?.trim().parse().ok()?;
            let devnum: u32 = fs::read_to_string(devnum_path).ok()?.trim().parse().ok()?;
            return Some(PathBuf::from(format!(
                "/dev/bus/usb/{busnum:03}/{devnum:03}"
            )));
        }
        current = dir.parent();
    }
    None
}

fn read_sysfs_usb_strings(sysfs_hid_path: &Path) -> UsbDeviceDescriptors {
    let mut current = Some(sysfs_hid_path);
    while let Some(dir) = current {
        if dir.join("busnum").exists() && dir.join("idVendor").exists() {
            return UsbDeviceDescriptors {
                manufacturer: fs::read_to_string(dir.join("manufacturer"))
                    .ok()
                    .map(|s| s.trim().to_string()),
                product_name: fs::read_to_string(dir.join("product"))
                    .ok()
                    .map(|s| s.trim().to_string()),
                serial_number: fs::read_to_string(dir.join("serial"))
                    .ok()
                    .map(|s| s.trim().to_string()),
                ..UsbDeviceDescriptors::default()
            };
        }
        current = dir.parent();
    }
    UsbDeviceDescriptors::default()
}
