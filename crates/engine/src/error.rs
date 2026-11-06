//! Errors returned directly to callers of the engine.
//!
//! Problems during a transfer don't appear here. They are recorded on the download
//! and reported through its state, because they happen long after the call returns.

use crate::scheduler::QueueId;
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
    #[error("the main queue can't be deleted")]
    MainQueue,
    #[error("queue {} does not exist", .0.0)]
    QueueNotFound(QueueId),
    #[error("{0}")]
    Request(String),
    #[error("{}", crate::remote::LOGIN_NEEDED)]
    LoginRequired,
    #[error("the proxy address isn't valid: {0}")]
    InvalidProxy(String),
    #[error("the database was created by a newer version of this program")]
    NewerDatabase,
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("could not start the HTTP client: {0}")]
    Client(#[from] reqwest::Error),
}
