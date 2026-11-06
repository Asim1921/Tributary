//! Errors returned directly to callers of the engine.
//!
//! Problems during a transfer don't appear here. They are recorded on the download
//! and reported through its state, because they happen long after the call returns.

use crate::store::DownloadId;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("`{0}` is not a valid http or https URL")]
    InvalidUrl(String),
    #[error("the header `{0}` is not valid")]
    InvalidHeader(String),
    #[error("download {} does not exist", .0.0)]
    NotFound(DownloadId),
    #[error("download {} is already running", .0.0)]
    AlreadyRunning(DownloadId),
    #[error("download {} has already completed", .0.0)]
    AlreadyCompleted(DownloadId),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("could not start the HTTP client: {0}")]
    Client(#[from] reqwest::Error),
}
