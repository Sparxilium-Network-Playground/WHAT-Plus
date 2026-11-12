//! 連到伺服器的 SSH 用戶端（來源端與管理客戶端共用）。
//!
//! 伺服器的 host key 採 TOFU：沒有記錄過指紋時，連線會以 [`ConnectError::UnknownHost`]
//! 失敗並帶回伺服器指紋，由使用者確認後再帶著指紋重新連線；指紋不符就拒絕連線。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, AuthResult};
use russh::keys::{PrivateKey, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{ChannelStream, Disconnect};
use whatplus_proto::{
    FrameError, PROTOCOL_VERSION, Request, Response, Role, SSH_USER, SUBSYSTEM, read_frame,
    write_frame,
};

use crate::keys::fingerprint;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    /// 第一次連線：請使用者確認指紋後重試。
    #[error("unknown server, fingerprint {fingerprint}")]
    UnknownHost { fingerprint: String },
    #[error("server fingerprint changed: expected {expected}, got {actual}")]
    HostKeyChanged { expected: String, actual: String },
    #[error("the server rejected this key (not registered, disabled, or IP not allowed)")]
    AuthRejected,
    #[error("connection failed: {0}")]
    Network(String),
    #[error("timed out")]
    Timeout,
    #[error("server error: {0}")]
    Server(String),
    #[error("protocol error: {0}")]
    Protocol(String),
}

impl From<FrameError> for ConnectError {
    fn from(e: FrameError) -> Self {
        match e {
            FrameError::Io(e) => ConnectError::Network(e.to_string()),
            other => ConnectError::Protocol(other.to_string()),
        }
    }
}

#[derive(Clone)]
pub struct ConnectOptions {
    pub host: String,
    pub port: u16,
    pub key: Arc<PrivateKey>,
    /// 已確認的伺服器指紋；`None` 表示第一次連線。
    pub known_fingerprint: Option<String>,
    /// 給伺服器 log 用的用戶端名稱，例如 `whatplus-agent/0.1.0`。
    pub client_name: String,
}

struct Handler {
    expected: Option<String>,
    observed: Arc<Mutex<Option<String>>>,
}

impl client::Handler for Handler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = server_key else {
            return Ok(false);
        };
        let actual = fingerprint(key);
        *self.observed.lock().unwrap() = Some(actual.clone());
        Ok(self.expected.as_deref() == Some(actual.as_str()))
    }
}

/// 已驗證、已進入 subsystem 的連線。
pub struct Connection {
    handle: client::Handle<Handler>,
    stream: ChannelStream<client::Msg>,
    pub role: Role,
    pub name: String,
    pub server_version: String,
    pub fingerprint: String,
}

impl Connection {
    pub async fn connect(opts: ConnectOptions) -> Result<Self, ConnectError> {
        let observed = Arc::new(Mutex::new(None));
        let handler = Handler {
            expected: opts.known_fingerprint.clone(),
            observed: observed.clone(),
        };
        let config = Arc::new(client::Config {
            inactivity_timeout: Some(Duration::from_secs(300)),
            ..Default::default()
        });
        let addr = (opts.host.as_str(), opts.port);
        let connected = tokio::time::timeout(CONNECT_TIMEOUT, client::connect(config, addr, handler))
            .await
            .map_err(|_| ConnectError::Timeout)?;
        let mut handle = match connected {
            Ok(handle) => handle,
            Err(e) => {
                let seen = observed.lock().unwrap().clone();
                return Err(match (seen, opts.known_fingerprint) {
                    (Some(actual), None) => ConnectError::UnknownHost {
                        fingerprint: actual,
                    },
                    (Some(actual), Some(expected)) if actual != expected => {
                        ConnectError::HostKeyChanged { expected, actual }
                    }
                    _ => ConnectError::Network(describe_network_error(&e)),
                });
            }
        };
        let fp = observed.lock().unwrap().clone().unwrap_or_default();

        let auth = handle
            .authenticate_publickey(SSH_USER, PrivateKeyWithHashAlg::new(opts.key.clone(), None))
            .await
            .map_err(|e| ConnectError::Network(e.to_string()))?;
        if !matches!(auth, AuthResult::Success) {
            return Err(ConnectError::AuthRejected);
        }

        let channel = handle
            .channel_open_session()
            .await
            .map_err(|e| ConnectError::Protocol(e.to_string()))?;
        channel
            .request_subsystem(true, SUBSYSTEM)
            .await
            .map_err(|e| ConnectError::Protocol(e.to_string()))?;
        let mut conn = Connection {
            handle,
            stream: channel.into_stream(),
            role: Role::Agent,
            name: String::new(),
            server_version: String::new(),
            fingerprint: fp,
        };
        match conn
            .request(Request::Hello {
                client: opts.client_name,
                protocol: PROTOCOL_VERSION,
            })
            .await?
        {
            Response::Welcome {
                role,
                name,
                server_version,
                ..
            } => {
                conn.role = role;
                conn.name = name;
                conn.server_version = server_version;
                Ok(conn)
            }
            other => Err(unexpected(other)),
        }
    }

    /// 送出請求並等待回應。伺服器回傳 `Error` 時轉成 [`ConnectError::Server`]。
    pub async fn request(&mut self, request: Request) -> Result<Response, ConnectError> {
        let exchange = async {
            write_frame(&mut self.stream, &request).await?;
            read_frame::<_, Response>(&mut self.stream).await
        };
        let response = tokio::time::timeout(REQUEST_TIMEOUT, exchange)
            .await
            .map_err(|_| ConnectError::Timeout)??;
        match response {
            None => Err(ConnectError::Network("server closed the connection".into())),
            Some(Response::Error { message }) => Err(ConnectError::Server(message)),
            Some(other) => Ok(other),
        }
    }

    pub async fn close(self) {
        let _ = self
            .handle
            .disconnect(Disconnect::ByApplication, "bye", "en")
            .await;
    }
}

/// 伺服器回了預期以外的回應。
pub fn unexpected(response: Response) -> ConnectError {
    ConnectError::Protocol(format!("unexpected response: {response:?}"))
}

fn describe_network_error(e: &russh::Error) -> String {
    match e {
        russh::Error::IO(io)
            if matches!(
                io.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::UnexpectedEof
                    | std::io::ErrorKind::ConnectionAborted
            ) =>
        {
            format!("{io} (the server may not allow this IP address)")
        }
        other => other.to_string(),
    }
}
