//! 伺服器、來源端（agent）與管理客戶端之間的協定。
//!
//! 所有訊息都在已驗證的 SSH subsystem [`SUBSYSTEM`] 內傳送，格式為
//! 4 bytes big-endian 長度 + JSON（見 [`frame`]）。一問一答，依序處理。

pub mod frame;
pub mod message;
pub mod types;

pub use frame::{FrameError, MAX_FRAME_BYTES, read_frame, write_frame};
pub use message::{Request, Response};
pub use types::*;

/// SSH subsystem 名稱。伺服器拒絕其他所有 subsystem、shell、exec 與轉發。
pub const SUBSYSTEM: &str = "whatplus";

/// 協定版本；不相容的變更要加一。
pub const PROTOCOL_VERSION: u32 = 1;

/// 預設 SSH port。
pub const DEFAULT_PORT: u16 = 33445;

/// SSH 登入時使用的使用者名稱（身分由公鑰決定，名稱本身不代表任何權限）。
pub const SSH_USER: &str = "whatplus";

/// 單次上傳的紀錄上限；來源端會分批送出。
pub const MAX_UPLOAD_BATCH: usize = 2000;
