use std::io;
use std::path::PathBuf;
use thiserror::Error;

/// Erros centrais do driver user-space UGEE.
#[derive(Debug, Error)]
pub enum DriverError {
    /// Nenhum dispositivo HID compatível com VID `0x28BD` foi localizado.
    #[error("nenhuma mesa UGEE (VID 0x28BD) compatível encontrada no barramento HID")]
    DeviceNotFound,

    /// O dispositivo foi desconectado durante a leitura contínua.
    #[error("dispositivo HID desconectado")]
    DeviceDisconnected,

    /// Falha de E/S ao interagir com arquivos sysfs, hidraw, usbfs ou uinput.
    #[error("erro de E/S em {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// Falha em chamada `ioctl` de kernel (`hidraw` ou `usbfs`).
    #[error("falha de ioctl ({op}) em {path}: {source}")]
    Ioctl {
        op: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// Pacote HID recebido possui tamanho insuficiente ou cabeçalho incompatível.
    #[error("pacote HID inválido: {0}")]
    InvalidPacket(String),

    /// Descritor USB binário (ex.: 0x64 ou 0x6E) malformado ou ausente.
    #[error("descritor USB inválido ({index:#04x}): {reason}")]
    InvalidDescriptor { index: u8, reason: String },

    /// Erro ao ler, validar ou serializar o arquivo de configuração TOML.
    #[error("erro de configuração em {path}: {reason}")]
    Config { path: PathBuf, reason: String },

    /// Erro ao criar ou emitir eventos via `/dev/uinput`.
    #[error("falha no subsistema uinput: {0}")]
    Uinput(#[from] io::Error),

    /// Falha de comunicação ou protocolo no canal IPC (Unix Domain Socket).
    #[error("falha no canal IPC: {0}")]
    Ipc(String),
}

/// Alias padrão de resultado para os módulos internos do driver.
pub type DriverResult<T> = Result<T, DriverError>;

impl DriverError {
    /// Retorna `true` se o erro for causado por falta de permissão (`EACCES`/`EPERM`).
    pub fn is_permission_denied(&self) -> bool {
        match self {
            Self::Io { source, .. } | Self::Ioctl { source, .. } | Self::Uinput(source) => {
                source.kind() == io::ErrorKind::PermissionDenied
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_permission_denied_variants() {
        let io_perm = DriverError::Io {
            path: PathBuf::from("/dev/hidraw7"),
            source: io::Error::from(io::ErrorKind::PermissionDenied),
        };
        assert!(io_perm.is_permission_denied());

        let not_found = DriverError::DeviceNotFound;
        assert!(!not_found.is_permission_denied());
    }
}
