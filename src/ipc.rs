use crate::config::DriverConfig;
use crate::error::{DriverError, DriverResult};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;
use std::time::Duration;
use tracing::{debug, warn};

/// Telemetria instantânea da caneta e dos botões da mesa.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveTelemetry {
    pub in_range: bool,
    pub tip_switch: bool,
    pub barrel_1: bool,
    pub barrel_2: bool,
    pub eraser: bool,
    pub raw_x: u32,
    pub raw_y: u32,
    pub mapped_x: i32,
    pub mapped_y: i32,
    pub norm_x: f32,
    pub norm_y: f32,
    pub inside_area: bool,
    pub precision_mode: bool,
    pub raw_pressure: u16,
    pub mapped_pressure: u16,
    pub tilt_x: i8,
    pub tilt_y: i8,
    pub keys_mask: u64,
    pub ring_index: u8,
    pub ring_delta: i8,
}

/// Snapshot completo de hardware, configuração e telemetria exposto via IPC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonSnapshot {
    pub connected: bool,
    pub hidraw_path: String,
    pub vid: u16,
    pub pid: u16,
    pub manufacturer: String,
    pub product_name: String,
    pub serial_number: String,
    pub ic_version: String,
    pub profile_name: String,
    pub max_x: u32,
    pub max_y: u32,
    pub max_pressure: u16,
    pub lpi: u16,
    pub express_keys_count: u8,
    pub battery_level: Option<u8>,
    pub led_brightness: Option<u8>,
    pub packets_processed: u64,
    pub telemetry: LiveTelemetry,
    pub config: DriverConfig,
    pub config_path: PathBuf,
}

impl Default for DaemonSnapshot {
    fn default() -> Self {
        Self {
            connected: false,
            hidraw_path: String::new(),
            vid: 0,
            pid: 0,
            manufacturer: String::new(),
            product_name: String::new(),
            serial_number: String::new(),
            ic_version: String::new(),
            profile_name: "M708".to_string(),
            max_x: 50794,
            max_y: 30474,
            max_pressure: 8191,
            lpi: 5080,
            express_keys_count: 8,
            battery_level: None,
            led_brightness: None,
            packets_processed: 0,
            telemetry: LiveTelemetry::default(),
            config: DriverConfig::default(),
            config_path: DriverConfig::default_config_path(),
        }
    }
}

/// Requisições JSON por linha aceitas pelo servidor IPC do daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum IpcRequest {
    GetStatus,
    UpdateConfig {
        config: Box<DriverConfig>,
        save_to_disk: bool,
    },
    ReloadConfig,
    SetLedBrightness {
        val1: u8,
        val2: Option<u8>,
    },
    TriggerKey {
        index: usize,
    },
}

/// Respostas JSON por linha retornadas pelo servidor IPC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum IpcResponse {
    Status(Box<DaemonSnapshot>),
    Ok { message: String },
    Error { message: String },
}

/// Mensagem de controle enviada da thread IPC para o loop principal do daemon.
#[derive(Debug)]
pub enum DaemonControlMsg {
    UpdateConfig {
        config: Box<DriverConfig>,
        save_to_disk: bool,
        reply: mpsc::SyncSender<IpcResponse>,
        canceled: Arc<AtomicBool>,
    },
    ReloadConfig {
        reply: mpsc::SyncSender<IpcResponse>,
        canceled: Arc<AtomicBool>,
    },
    SetLedBrightness {
        val1: u8,
        val2: Option<u8>,
        reply: mpsc::SyncSender<IpcResponse>,
        canceled: Arc<AtomicBool>,
    },
    TriggerKey {
        index: usize,
        reply: mpsc::SyncSender<IpcResponse>,
        canceled: Arc<AtomicBool>,
    },
}

impl DaemonControlMsg {
    /// Indica se o remetente já expirou no timeout IPC e cancelou a aplicação desta mensagem.
    pub fn is_canceled(&self) -> bool {
        match self {
            Self::UpdateConfig { canceled, .. }
            | Self::ReloadConfig { canceled, .. }
            | Self::SetLedBrightness { canceled, .. }
            | Self::TriggerKey { canceled, .. } => canceled.load(Ordering::SeqCst),
        }
    }
}

/// Caminho padrão do socket Unix IPC (`$XDG_RUNTIME_DIR/ugee-driver.sock`).
pub fn default_socket_path() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join("ugee-driver.sock");
        }
    }
    PathBuf::from("/tmp/ugee-driver.sock")
}

/// Servidor IPC em Unix Domain Socket (JSON por linha).
pub struct IpcServer {
    listener: UnixListener,
    socket_path: PathBuf,
}

impl IpcServer {
    /// Abre e escuta no caminho de socket informado, removendo socket órfão anterior se inativo.
    pub fn bind(socket_path: PathBuf) -> DriverResult<Self> {
        if let Some(parent) = socket_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|source| DriverError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
        }

        if socket_path.exists() {
            if UnixStream::connect(&socket_path).is_ok() {
                return Err(DriverError::Ipc(format!(
                    "já existe um servidor IPC ativo em {}",
                    socket_path.display()
                )));
            }
            fs::remove_file(&socket_path).map_err(|source| DriverError::Io {
                path: socket_path.clone(),
                source,
            })?;
        }

        let listener = UnixListener::bind(&socket_path).map_err(|source| DriverError::Io {
            path: socket_path.clone(),
            source,
        })?;
        listener
            .set_nonblocking(true)
            .map_err(|source| DriverError::Io {
                path: socket_path.clone(),
                source,
            })?;

        Ok(Self {
            listener,
            socket_path,
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Aceita uma única conexão pendente no listener não-bloqueante, se houver.
    pub fn try_accept(&self) -> DriverResult<Option<UnixStream>> {
        loop {
            match self.listener.accept() {
                Ok((stream, _addr)) => return Ok(Some(stream)),
                Err(err) if err.kind() == ErrorKind::WouldBlock => return Ok(None),
                Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(source) => {
                    return Err(DriverError::Io {
                        path: self.socket_path.clone(),
                        source,
                    });
                }
            }
        }
    }

    /// Processa todas as conexões pendentes sem bloquear quando não há clientes.
    pub fn accept_and_handle<F>(&self, mut handler: F) -> DriverResult<usize>
    where
        F: FnMut(IpcRequest) -> IpcResponse,
    {
        let mut handled = 0usize;
        while let Some(stream) = self.try_accept()? {
            handled += 1;
            if let Err(err) = handle_client_stream(stream, None, &mut handler) {
                debug!(error = %err, "erro ao atender cliente IPC");
            }
        }
        Ok(handled)
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket_path);
    }
}

fn handle_client_stream<F>(
    stream: UnixStream,
    running: Option<&AtomicBool>,
    mut handler: F,
) -> DriverResult<()>
where
    F: FnMut(IpcRequest) -> IpcResponse,
{
    let _ = stream.set_nonblocking(false);
    let read_timeout = if running.is_some() {
        Duration::from_millis(250)
    } else {
        Duration::from_millis(500)
    };
    let _ = stream.set_read_timeout(Some(read_timeout));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));

    let writer_stream = stream
        .try_clone()
        .map_err(|source| DriverError::Ipc(source.to_string()))?;
    let mut reader = BufReader::new(stream);
    let mut writer = writer_stream;
    let mut line = String::new();

    loop {
        if let Some(flag) = running {
            if !flag.load(Ordering::SeqCst) {
                break;
            }
        }

        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let response = match serde_json::from_str::<IpcRequest>(trimmed) {
                    Ok(req) => handler(req),
                    Err(err) => IpcResponse::Error {
                        message: format!("JSON inválido na requisição IPC: {err}"),
                    },
                };

                let mut payload = serde_json::to_string(&response).map_err(|err| {
                    DriverError::Ipc(format!("erro serializando resposta IPC: {err}"))
                })?;
                payload.push('\n');
                writer
                    .write_all(payload.as_bytes())
                    .and_then(|()| writer.flush())
                    .map_err(|err| DriverError::Ipc(format!("erro enviando resposta IPC: {err}")))?;
            }
            Err(err)
                if matches!(
                    err.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                ) =>
            {
                if running.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
                    continue;
                }
                break;
            }
            Err(err) => {
                return Err(DriverError::Ipc(format!("erro lendo requisição: {err}")));
            }
        }
    }

    Ok(())
}

/// Envia uma requisição JSON síncrona para o daemon e retorna a resposta.
pub fn send_ipc_request(socket_path: &Path, request: &IpcRequest) -> DriverResult<IpcResponse> {
    let mut stream = UnixStream::connect(socket_path).map_err(|source| DriverError::Io {
        path: socket_path.to_path_buf(),
        source,
    })?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));

    let mut encoded = serde_json::to_string(request)
        .map_err(|err| DriverError::Ipc(format!("erro serializando requisição: {err}")))?;
    encoded.push('\n');

    stream
        .write_all(encoded.as_bytes())
        .and_then(|()| stream.flush())
        .map_err(|source| DriverError::Io {
            path: socket_path.to_path_buf(),
            source,
        })?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .map_err(|source| DriverError::Io {
            path: socket_path.to_path_buf(),
            source,
        })?;
    if n == 0 {
        return Err(DriverError::Ipc(
            "daemon encerrou a conexão sem enviar resposta".to_string(),
        ));
    }

    serde_json::from_str::<IpcResponse>(line.trim())
        .map_err(|err| DriverError::Ipc(format!("resposta IPC inválida: {err}")))
}

/// Aplica uma atualização de configuração diretamente no `snapshot` (e opcionalmente no TOML)
/// enquanto nenhuma mesa estiver conectada.
pub fn apply_offline_config(
    snapshot: &Arc<RwLock<DaemonSnapshot>>,
    config: DriverConfig,
    save_to_disk: bool,
) -> IpcResponse {
    let path = snapshot
        .read()
        .map(|g| g.config_path.clone())
        .unwrap_or_else(|_| DriverConfig::default_config_path());
    if save_to_disk {
        if let Err(err) = config.save_to_path(&path) {
            return IpcResponse::Error {
                message: err.to_string(),
            };
        }
    }
    if let Ok(mut guard) = snapshot.write() {
        guard.config = config;
    }
    IpcResponse::Ok {
        message: "configuração atualizada (aguardando mesa conectada)".to_string(),
    }
}

/// Recarrega o arquivo TOML em `snapshot.config_path` enquanto nenhuma mesa estiver conectada.
pub fn reload_offline_config(snapshot: &Arc<RwLock<DaemonSnapshot>>) -> IpcResponse {
    let path = snapshot
        .read()
        .map(|g| g.config_path.clone())
        .unwrap_or_else(|_| DriverConfig::default_config_path());
    match DriverConfig::load_from_path(&path) {
        Ok(cfg) => {
            if let Ok(mut guard) = snapshot.write() {
                guard.config = cfg;
            }
            IpcResponse::Ok {
                message: format!("configuração recarregada de {}", path.display()),
            }
        }
        Err(err) => IpcResponse::Error {
            message: err.to_string(),
        },
    }
}

/// Drena mensagens pendentes de `control_rx` enquanto o dispositivo estiver desconectado,
/// descartando mensagens já expiradas e tratando o restante no estado offline.
pub fn drain_offline_control_messages(
    snapshot: &Arc<RwLock<DaemonSnapshot>>,
    control_rx: &mpsc::Receiver<DaemonControlMsg>,
) {
    while let Ok(msg) = control_rx.try_recv() {
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
                let resp = apply_offline_config(snapshot, *config, save_to_disk);
                let _ = reply.send(resp);
            }
            DaemonControlMsg::ReloadConfig { reply, .. } => {
                let resp = reload_offline_config(snapshot);
                let _ = reply.send(resp);
            }
            DaemonControlMsg::SetLedBrightness { reply, .. }
            | DaemonControlMsg::TriggerKey { reply, .. } => {
                let _ = reply.send(IpcResponse::Error {
                    message: "mesa não conectada no momento".to_string(),
                });
            }
        }
    }
}

fn process_ipc_request(
    snapshot: &Arc<RwLock<DaemonSnapshot>>,
    control_tx: &mpsc::Sender<DaemonControlMsg>,
    req: IpcRequest,
) -> IpcResponse {
    match req {
        IpcRequest::GetStatus => match snapshot.read() {
            Ok(guard) => IpcResponse::Status(Box::new(guard.clone())),
            Err(_) => IpcResponse::Error {
                message: "falha interna ao ler estado do daemon".to_string(),
            },
        },
        IpcRequest::UpdateConfig {
            config,
            save_to_disk,
        } => {
            let config_for_ctrl = config.clone();
            let snapshot_fallback = Arc::clone(snapshot);
            dispatch_control_or_fallback(
                snapshot,
                control_tx,
                move |reply, canceled| DaemonControlMsg::UpdateConfig {
                    config: config_for_ctrl,
                    save_to_disk,
                    reply,
                    canceled,
                },
                move || apply_offline_config(&snapshot_fallback, *config, save_to_disk),
            )
        }
        IpcRequest::ReloadConfig => {
            let snapshot_fallback = Arc::clone(snapshot);
            dispatch_control_or_fallback(
                snapshot,
                control_tx,
                |reply, canceled| DaemonControlMsg::ReloadConfig { reply, canceled },
                move || reload_offline_config(&snapshot_fallback),
            )
        }
        IpcRequest::SetLedBrightness { val1, val2 } => dispatch_control_or_fallback(
            snapshot,
            control_tx,
            |reply, canceled| DaemonControlMsg::SetLedBrightness {
                val1,
                val2,
                reply,
                canceled,
            },
            || IpcResponse::Error {
                message: "mesa não conectada no momento".to_string(),
            },
        ),
        IpcRequest::TriggerKey { index } => dispatch_control_or_fallback(
            snapshot,
            control_tx,
            |reply, canceled| DaemonControlMsg::TriggerKey {
                index,
                reply,
                canceled,
            },
            || IpcResponse::Error {
                message: "mesa não conectada no momento".to_string(),
            },
        ),
    }
}

/// Inicia uma thread dedicada para servir o socket Unix IPC sem bloquear o loop HID,
/// permitindo múltiplas conexões persistentes simultâneas (GUI) e chamadas CLI rápidas.
pub fn spawn_ipc_thread(
    server: IpcServer,
    snapshot: Arc<RwLock<DaemonSnapshot>>,
    control_tx: mpsc::Sender<DaemonControlMsg>,
    running: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut client_handles: Vec<JoinHandle<()>> = Vec::new();
        while running.load(Ordering::SeqCst) {
            client_handles.retain(|h| !h.is_finished());
            match server.try_accept() {
                Ok(Some(stream)) => {
                    let snap = Arc::clone(&snapshot);
                    let tx = control_tx.clone();
                    let run = Arc::clone(&running);
                    let handle = std::thread::spawn(move || {
                        if let Err(err) = handle_client_stream(stream, Some(&run), |req| {
                            process_ipc_request(&snap, &tx, req)
                        }) {
                            debug!(error = %err, "cliente IPC persistente encerrado");
                        }
                    });
                    client_handles.push(handle);
                }
                Ok(None) => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(err) => {
                    warn!(error = %err, "erro no loop do servidor IPC");
                    break;
                }
            }
        }
        for handle in client_handles {
            let _ = handle.join();
        }
    })
}

fn dispatch_control_or_fallback<M, F>(
    snapshot: &Arc<RwLock<DaemonSnapshot>>,
    control_tx: &mpsc::Sender<DaemonControlMsg>,
    make_msg: M,
    offline_fallback: F,
) -> IpcResponse
where
    M: FnOnce(mpsc::SyncSender<IpcResponse>, Arc<AtomicBool>) -> DaemonControlMsg,
    F: FnOnce() -> IpcResponse,
{
    let is_connected = snapshot.read().map(|g| g.connected).unwrap_or(false);
    if !is_connected {
        return offline_fallback();
    }

    let (reply_tx, reply_rx) = mpsc::sync_channel(1);
    let canceled = Arc::new(AtomicBool::new(false));
    if control_tx
        .send(make_msg(reply_tx, Arc::clone(&canceled)))
        .is_err()
    {
        return offline_fallback();
    }

    match reply_rx.recv_timeout(Duration::from_millis(600)) {
        Ok(resp) => resp,
        Err(mpsc::RecvTimeoutError::Disconnected) => offline_fallback(),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            canceled.store(true, Ordering::SeqCst);
            let still_connected = snapshot.read().map(|g| g.connected).unwrap_or(false);
            if !still_connected {
                offline_fallback()
            } else {
                IpcResponse::Error {
                    message: "tempo limite aguardando resposta do loop USB".to_string(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_server_and_client_roundtrip_over_unix_socket() -> Result<(), Box<dyn std::error::Error>>
    {
        let sock_path = std::env::temp_dir().join(format!(
            "ugee-driver-test-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let server = IpcServer::bind(sock_path.clone())?;

        let client_path = sock_path.clone();
        let client_handle =
            std::thread::spawn(move || send_ipc_request(&client_path, &IpcRequest::GetStatus));

        let mut handled = 0usize;
        for _ in 0..50 {
            handled += server.accept_and_handle(|req| match req {
                IpcRequest::GetStatus => {
                    let mut snap = DaemonSnapshot {
                        connected: true,
                        product_name: "M708".to_string(),
                        ..DaemonSnapshot::default()
                    };
                    snap.telemetry.raw_pressure = 4096;
                    IpcResponse::Status(Box::new(snap))
                }
                _ => IpcResponse::Ok {
                    message: "ok".to_string(),
                },
            })?;
            if handled > 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        let resp = client_handle
            .join()
            .map_err(|_| "thread cliente IPC falhou")??;
        match resp {
            IpcResponse::Status(snap) => {
                assert!(snap.connected);
                assert_eq!(snap.product_name, "M708");
                assert_eq!(snap.telemetry.raw_pressure, 4096);
            }
            other => return Err(format!("resposta inesperada: {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn canceled_and_offline_drained_messages_do_not_overwrite_newer_config(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let stale_cfg = DriverConfig {
            mouse_speed: 1,
            ..DriverConfig::default()
        };

        let newer_cfg = DriverConfig {
            mouse_speed: 9,
            ..DriverConfig::default()
        };

        let snapshot = Arc::new(RwLock::new(DaemonSnapshot {
            connected: false,
            config: newer_cfg,
            ..DaemonSnapshot::default()
        }));
        let (tx, rx) = mpsc::channel();

        let (reply_stale_tx, _reply_stale_rx) = mpsc::sync_channel(1);
        tx.send(DaemonControlMsg::UpdateConfig {
            config: Box::new(stale_cfg),
            save_to_disk: false,
            reply: reply_stale_tx,
            canceled: Arc::new(AtomicBool::new(true)),
        })?;

        let active_cfg = DriverConfig {
            mouse_speed: 7,
            ..DriverConfig::default()
        };
        let (reply_ok_tx, reply_ok_rx) = mpsc::sync_channel(1);
        tx.send(DaemonControlMsg::UpdateConfig {
            config: Box::new(active_cfg),
            save_to_disk: false,
            reply: reply_ok_tx,
            canceled: Arc::new(AtomicBool::new(false)),
        })?;

        drain_offline_control_messages(&snapshot, &rx);
        assert!(matches!(reply_ok_rx.try_recv(), Ok(IpcResponse::Ok { .. })));
        let final_speed = snapshot
            .read()
            .map_err(|_| "rwlock envenenado")?
            .config
            .mouse_speed;
        assert_eq!(final_speed, 7);
        Ok(())
    }

    #[test]
    fn persistent_client_handles_multiple_requests_including_trigger_key(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let sock_path = std::env::temp_dir().join(format!(
            "ugee-driver-persist-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let server = IpcServer::bind(sock_path.clone())?;
        let snapshot = Arc::new(RwLock::new(DaemonSnapshot::default()));
        let (tx, _rx) = mpsc::channel();
        let running = Arc::new(AtomicBool::new(true));
        let handle = spawn_ipc_thread(server, Arc::clone(&snapshot), tx, Arc::clone(&running));

        let mut stream = UnixStream::connect(&sock_path)?;
        stream.set_read_timeout(Some(Duration::from_millis(800)))?;
        let mut reader = BufReader::new(stream.try_clone()?);

        for req in [
            IpcRequest::GetStatus,
            IpcRequest::TriggerKey { index: 2 },
            IpcRequest::GetStatus,
        ] {
            let mut enc = serde_json::to_string(&req)?;
            enc.push('\n');
            stream.write_all(enc.as_bytes())?;
            stream.flush()?;

            let mut resp_line = String::new();
            let n = reader.read_line(&mut resp_line)?;
            assert!(n > 0);
            let resp: IpcResponse = serde_json::from_str(resp_line.trim())?;
            match req {
                IpcRequest::GetStatus => assert!(matches!(resp, IpcResponse::Status(_))),
                IpcRequest::TriggerKey { .. } => assert!(matches!(resp, IpcResponse::Error { .. })),
                _ => {}
            }
        }

        drop(stream);
        running.store(false, Ordering::SeqCst);
        let _ = handle.join();
        Ok(())
    }
}
