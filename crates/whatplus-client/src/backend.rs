//! 背景 tokio runtime 與 SSH 連線。UI 只透過 [`Backend::spawn`] 交辦工作，結果回到 UI 執行緒。

use std::future::Future;
use std::sync::Arc;

use tokio::sync::Mutex;
use whatplus_proto::{Request, Response, Role};
use whatplus_ssh::client::{ConnectError, ConnectOptions, Connection};

struct Session {
    conn: Connection,
    opts: ConnectOptions,
}

pub struct Backend {
    rt: tokio::runtime::Runtime,
    session: Arc<Mutex<Option<Session>>>,
}

/// 連線成功的資訊。
#[derive(Debug, Clone)]
pub struct Connected {
    pub name: String,
    pub server_version: String,
}

impl Backend {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            rt: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()?,
            session: Arc::new(Mutex::new(None)),
        })
    }

    /// 在背景執行 `fut`，完成後在 UI 執行緒呼叫 `done`。
    pub fn spawn<T, F, D>(&self, fut: F, done: D)
    where
        T: Send + 'static,
        F: Future<Output = T> + Send + 'static,
        D: FnOnce(T) + Send + 'static,
    {
        self.rt.spawn(async move {
            let value = fut.await;
            let _ = slint::invoke_from_event_loop(move || done(value));
        });
    }

    /// 建立連線（取代舊連線）。
    pub fn connect(&self, opts: ConnectOptions) -> impl Future<Output = Result<Connected, ConnectError>> + Send + 'static {
        let session = self.session.clone();
        async move {
            let old = session.lock().await.take();
            if let Some(old) = old {
                old.conn.close().await;
            }
            let conn = Connection::connect(opts.clone()).await?;
            let info = Connected {
                name: conn.name.clone(),
                server_version: conn.server_version.clone(),
            };
            if conn.role != Role::Admin {
                conn.close().await;
                return Err(ConnectError::Server(format!(
                    "\"{}\" is registered as an agent key; the client needs an admin key",
                    info.name
                )));
            }
            *session.lock().await = Some(Session { conn, opts });
            Ok(info)
        }
    }

    pub fn disconnect(&self) -> impl Future<Output = ()> + Send + 'static {
        let session = self.session.clone();
        async move {
            if let Some(s) = session.lock().await.take() {
                s.conn.close().await;
            }
        }
    }

    /// 送出請求。連線因閒置被關閉時自動重連一次（沿用已確認的指紋）。
    pub fn request(&self, req: Request) -> impl Future<Output = Result<Response, String>> + Send + 'static {
        let session = self.session.clone();
        async move {
            let mut guard = session.lock().await;
            let Some(s) = guard.as_mut() else {
                return Err("not connected".to_string());
            };
            match s.conn.request(req.clone()).await {
                Ok(r) => Ok(r),
                Err(ConnectError::Network(_) | ConnectError::Timeout) => {
                    let opts = s.opts.clone();
                    let conn = Connection::connect(opts.clone()).await.map_err(|e| e.to_string())?;
                    *guard = Some(Session { conn, opts });
                    let s = guard.as_mut().expect("just set");
                    s.conn.request(req).await.map_err(|e| e.to_string())
                }
                Err(e) => Err(e.to_string()),
            }
        }
    }
}
