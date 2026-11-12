//! SSH 共用程式碼。
//!
//! - [`keys`]：公鑰／私鑰的掃描、解析、指紋與產生（只支援 Ed25519）。
//! - [`allow`]：IP 白名單（CIDR）。
//! - [`client`]：來源端與管理客戶端連到伺服器的用戶端（host key TOFU）。

pub mod allow;
pub mod client;
pub mod keys;

pub use russh::keys::{PrivateKey, PublicKey};
