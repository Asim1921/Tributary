//! Segmented, resumable HTTP downloads. Windows only.
//!
//! A download is split across several connections. Progress is saved to SQLite
//! only after the matching data has been flushed to disk, so after a crash the
//! database never claims bytes the file doesn't have.

mod auth;
mod disk;
mod engine;
mod error;
mod ftp;
mod limits;
pub mod name;
mod parts;
mod remote;
mod scheduler;
pub mod segments;
mod store;
mod transfer;
mod wildcard;

pub use auth::Login;
pub use engine::{
    Config, DownloadEdit, Engine, Event, FileInfo, ManualProxy, NetworkOptions, NewDownload,
    ProxyMode, QueueInfo, ServerLimit,
};
pub use error::Error;
pub use limits::Quota;
pub use remote::LOGIN_NEEDED;
pub use scheduler::{EVERY_DAY, Moment, Queue, QueueId, QueueKind, RETRY_DELAY_SECONDS};
pub use store::{Download, DownloadId, State};
pub use transfer::RetryPolicy;
pub use wildcard::wildcard_match;
