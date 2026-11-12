//! subsystem 內的一問一答迴圈。

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use russh::ChannelStream;
use russh::server::Msg;
use whatplus_proto::{PROTOCOL_VERSION, Request, Response, read_frame, write_frame};

use crate::api;
use crate::app::App;
use crate::auth::Identity;

const HELLO_TIMEOUT: Duration = Duration::from_secs(30);
/// 兩個請求之間最長的閒置時間。
const IDLE_TIMEOUT: Duration = Duration::from_secs(600);

pub async fn serve(app: Arc<App>, who: Identity, peer: Option<SocketAddr>, mut stream: ChannelStream<Msg>) {
    match tokio::time::timeout(HELLO_TIMEOUT, read_frame::<_, Request>(&mut stream)).await {
        Ok(Ok(Some(Request::Hello { client, protocol }))) => {
            if protocol != PROTOCOL_VERSION {
                let _ = write_frame(
                    &mut stream,
                    &Response::error(format!(
                        "protocol version {protocol} is not supported (server uses {PROTOCOL_VERSION}); update the client"
                    )),
                )
                .await;
                return;
            }
            tracing::info!(?peer, name = %who.name, client = %truncate(&client), "session started");
            let welcome = Response::Welcome {
                role: who.role,
                name: who.name.clone(),
                server_version: env!("CARGO_PKG_VERSION").to_string(),
                protocol: PROTOCOL_VERSION,
            };
            if write_frame(&mut stream, &welcome).await.is_err() {
                return;
            }
        }
        _ => {
            tracing::warn!(?peer, "session closed: no valid hello");
            return;
        }
    }

    loop {
        let request = match tokio::time::timeout(IDLE_TIMEOUT, read_frame::<_, Request>(&mut stream)).await {
            Ok(Ok(Some(req))) => req,
            Ok(Ok(None)) | Err(_) => break,
            Ok(Err(e)) => {
                tracing::warn!(?peer, name = %who.name, "session error: {e}");
                break;
            }
        };
        let name = api::request_name(&request);
        let started = std::time::Instant::now();
        let response = api::handle(&app, &who, request).await;
        let ok = !matches!(response, Response::Error { .. });
        tracing::info!(name = %who.name, request = name, ok, ms = started.elapsed().as_millis() as u64, "request");
        if write_frame(&mut stream, &response).await.is_err() {
            break;
        }
    }
    tracing::info!(?peer, name = %who.name, "session ended");
}

fn truncate(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(64).collect()
}
