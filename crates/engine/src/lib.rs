//! Segmented, resumable HTTP downloads. Windows only.
//!
//! A download is split across several connections. Progress is saved to SQLite
//! only after the matching data has been flushed to disk, so after a crash the
//! database never claims bytes the file doesn't have.

mod disk;
mod engine;
mod error;
pub mod name;
mod remote;
pub mod segments;
mod store;
mod transfer;

pub use engine::{Config, Engine, Event, NewDownload};
pub use error::Error;
pub use store::{Download, DownloadId, State};
pub use transfer::RetryPolicy;
