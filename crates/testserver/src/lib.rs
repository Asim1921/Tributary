//! A local HTTP server for studying and testing download clients.
//!
//! HTTP is handled directly on the socket rather than through a framework. That
//! keeps the exact header case and order a client sends, which frameworks
//! normalise away, and allows a transfer to be cut off part way through.
//!
//! Files are computed from their offset, so large files use no disk space and
//! their checksums are known in advance. See [`scenario`] for the fault options.

pub mod content;
pub mod ftp;
pub mod http;
pub mod scenario;
pub mod server;
pub mod socks;

pub use ftp::{FtpConfig, FtpServer};
pub use server::{Config, Outcome, Record, TestServer};
pub use socks::SocksProxy;
