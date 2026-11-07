//! Explores a website, politely, for pages and files to save.
//!
//! Exploring is kept apart from the network: [`Crawl`] is a plain value holding where
//! exploration has got to, so a project can be saved and carried on later, and
//! [`explore`] drives it through a [`Source`]. Pages are read one at a time with a delay
//! between them, and `robots.txt` is always honoured.

mod crawl;
pub mod links;
pub mod offline;
mod project;
pub mod robots;

pub use crawl::{Crawl, Ended, Fetched, Found, SavedPage, Source, Target, explore};
pub use project::{Project, ProjectFile, Scope, Template};

/// How this crawler names itself to `robots.txt`.
pub const ROBOT_NAME: &str = "tributary";
