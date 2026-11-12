//! 每條 SSH 連線的 russh Handler。
//!
//! 只允許：Ed25519 公鑰登入 → 開一個 session channel → 進入 [`SUBSYSTEM`]。
//! shell、exec、pty、env、X11、agent 轉發、TCP 轉發一律拒絕（russh 預設也會拒絕轉發 channel）。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use russh::server::{
    self, Auth, ChannelOpenHandle, Msg, PublicKeyOffer, Session, VerifiedPublicKey,
};
use russh::{Channel, ChannelId};
use whatplus_proto::SUBSYSTEM;
use whatplus_ssh::keys;

use crate::app::App;
use crate::auth::Identity;

pub struct ConnHandler {
    app: Arc<App>,
    peer: Option<SocketAddr>,
    identity: Option<Identity>,
    pending: HashMap<ChannelId, Channel<Msg>>,
    subsystem_started: bool,
}

impl ConnHandler {
    pub fn new(app: Arc<App>, peer: Option<SocketAddr>) -> Self {
        Self {
            app,
            peer,
            identity: None,
            pending: HashMap::new(),
            subsystem_started: false,
        }
    }

    fn peer_ip(&self) -> Option<std::net::IpAddr> {
        self.peer.map(|p| p.ip())
    }

    fn deny(&self, channel: ChannelId, session: &mut Session, what: &str) -> anyhow::Result<()> {
        tracing::warn!(peer = ?self.peer, "rejected {what} request");
        session.channel_failure(channel)?;
        session.close(channel)?;
        Ok(())
    }
}

impl server::Handler for ConnHandler {
    type Error = anyhow::Error;

    async fn auth_publickey_offered(
        &mut self,
        _user: &str,
        offer: PublicKeyOffer<'_>,
    ) -> Result<Auth, Self::Error> {
        let fp = keys::fingerprint(offer.public_key);
        if keys::is_supported(offer.public_key) && self.app.auth.current().knows(&fp) {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn auth_publickey(
        &mut self,
        _user: &str,
        key: VerifiedPublicKey<'_>,
    ) -> Result<Auth, Self::Error> {
        if !keys::is_supported(key.public_key) {
            return Ok(Auth::reject());
        }
        let fp = keys::fingerprint(key.public_key);
        match self.app.auth.current().authenticate(&fp, self.peer_ip()) {
            Some(identity) => {
                tracing::info!(peer = ?self.peer, name = %identity.name, role = identity.role.as_str(), "authenticated");
                let ip = self.peer_ip().map(|i| i.to_string()).unwrap_or_default();
                let _ = self.app.store.record_activity(identity.key_id, &ip);
                self.identity = Some(identity);
                Ok(Auth::Accept)
            }
            None => {
                tracing::warn!(peer = ?self.peer, %fp, "public key rejected");
                Ok(Auth::reject())
            }
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // 每條連線只允許一個 channel。
        if self.identity.is_none() || self.subsystem_started || !self.pending.is_empty() {
            drop(reply); // 未回覆即視為拒絕
            return Ok(());
        }
        self.pending.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel_id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let (Some(identity), Some(channel)) =
            (self.identity.clone(), self.pending.remove(&channel_id))
        else {
            return self.deny(channel_id, session, "subsystem");
        };
        if name != SUBSYSTEM {
            return self.deny(channel_id, session, "unknown subsystem");
        }
        self.subsystem_started = true;
        session.channel_success(channel_id)?;
        let app = self.app.clone();
        let peer = self.peer;
        tokio::spawn(async move {
            super::session::serve(app, identity, peer, channel.into_stream()).await;
        });
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.deny(channel, session, "shell")
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        _data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.deny(channel, session, "exec")
    }

    #[allow(clippy::too_many_arguments)]
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.deny(channel, session, "pty")
    }

    async fn tcpip_forward(
        &mut self,
        _address: &str,
        _port: &mut u32,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        Ok(false)
    }
}
