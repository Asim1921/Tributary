//! Saved downloads and their progress.
//!
//! Secret headers such as cookies must never reach this module. The engine removes
//! them first, and a test checks the database file for them.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::remote::Remote;
use crate::segments::Segment;

const SCHEMA: &str = "
    PRAGMA journal_mode = WAL;
    PRAGMA synchronous = NORMAL;
    PRAGMA foreign_keys = ON;

    CREATE TABLE IF NOT EXISTS downloads (
        id            INTEGER PRIMARY KEY,
        url           TEXT NOT NULL,
        dest_dir      TEXT NOT NULL,
        file_name     TEXT,
        headers       TEXT NOT NULL,
        size          INTEGER,
        etag          TEXT,
        last_modified TEXT,
        ranged        INTEGER NOT NULL DEFAULT 0,
        state         TEXT NOT NULL,
        error         TEXT,
        final_path    TEXT
    );

    CREATE TABLE IF NOT EXISTS segments (
        download_id INTEGER NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
        seg_start   INTEGER NOT NULL,
        seg_end     INTEGER NOT NULL,
        written     INTEGER NOT NULL,
        PRIMARY KEY (download_id, seg_start)
    );
";

const SELECT_DOWNLOAD: &str = "
    SELECT d.id, d.url, d.dest_dir, d.file_name, d.headers, d.size, d.etag, d.last_modified,
           d.ranged, d.state, d.error, d.final_path,
           COALESCE((SELECT SUM(s.written) FROM segments s WHERE s.download_id = d.id), 0)
    FROM downloads d
";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DownloadId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Running,
    Paused,
    Completed,
    Failed,
}

impl State {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "running" => Self::Running,
            "paused" => Self::Paused,
            "completed" => Self::Completed,
            _ => Self::Failed,
        }
    }
}

/// A download as last saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub id: DownloadId,
    pub url: String,
    pub dest_dir: PathBuf,
    pub file_name: Option<String>,
    /// Headers safe to save. Secret ones are never included.
    pub headers: Vec<(String, String)>,
    pub size: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub ranged: bool,
    pub state: State,
    pub error: Option<String>,
    pub final_path: Option<PathBuf>,
    /// Bytes safely on disk as of the last checkpoint.
    pub downloaded: u64,
}

impl Download {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let headers: String = row.get(4)?;
        let size = row.get::<_, Option<i64>>(5)?.map(|size| size as u64);
        let state = State::parse(&row.get::<_, String>(9)?);
        let saved = row.get::<_, i64>(12)? as u64;

        Ok(Self {
            id: DownloadId(row.get(0)?),
            url: row.get(1)?,
            dest_dir: PathBuf::from(row.get::<_, String>(2)?),
            file_name: row.get(3)?,
            headers: serde_json::from_str(&headers).unwrap_or_default(),
            size,
            etag: row.get(6)?,
            last_modified: row.get(7)?,
            ranged: row.get(8)?,
            state,
            error: row.get(10)?,
            final_path: row.get::<_, Option<String>>(11)?.map(PathBuf::from),
            downloaded: match state {
                State::Completed => size.unwrap_or(saved),
                _ => saved,
            },
        })
    }
}

pub struct Store {
    connection: Mutex<Connection>,
}

impl Store {
    /// Opens the database. Anything left running belonged to a process that died,
    /// so it becomes paused and waits to be resumed.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let connection = Connection::open(path)?;
        connection.execute_batch(SCHEMA)?;
        connection.execute(
            "UPDATE downloads SET state = ?1 WHERE state = ?2",
            params![State::Paused.as_str(), State::Running.as_str()],
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub fn insert(
        &self,
        url: &str,
        dest_dir: &Path,
        file_name: Option<&str>,
        headers: &[(String, String)],
    ) -> rusqlite::Result<DownloadId> {
        let headers = serde_json::to_string(headers).unwrap_or_else(|_| "[]".to_owned());
        let connection = self.lock();
        connection.execute(
            "INSERT INTO downloads (url, dest_dir, file_name, headers, state)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                url,
                dest_dir.to_string_lossy(),
                file_name,
                headers,
                State::Paused.as_str()
            ],
        )?;
        Ok(DownloadId(connection.last_insert_rowid()))
    }

    pub fn get(&self, id: DownloadId) -> rusqlite::Result<Option<Download>> {
        self.lock()
            .query_row(
                &format!("{SELECT_DOWNLOAD} WHERE d.id = ?1"),
                params![id.0],
                Download::from_row,
            )
            .optional()
    }

    pub fn list(&self) -> rusqlite::Result<Vec<Download>> {
        let connection = self.lock();
        let mut statement = connection.prepare(&format!("{SELECT_DOWNLOAD} ORDER BY d.id"))?;
        statement.query_map([], Download::from_row)?.collect()
    }

    pub fn segments(&self, id: DownloadId) -> rusqlite::Result<Vec<Segment>> {
        let connection = self.lock();
        let mut statement = connection.prepare(
            "SELECT seg_start, seg_end, written FROM segments
             WHERE download_id = ?1 ORDER BY seg_start",
        )?;
        statement
            .query_map(params![id.0], |row| {
                Ok(Segment {
                    start: row.get::<_, i64>(0)? as u64,
                    end: row.get::<_, i64>(1)? as u64,
                    written: row.get::<_, i64>(2)? as u64,
                })
            })?
            .collect()
    }

    pub fn set_state(
        &self,
        id: DownloadId,
        state: State,
        error: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.lock().execute(
            "UPDATE downloads SET state = ?2, error = ?3 WHERE id = ?1",
            params![id.0, state.as_str(), error],
        )?;
        Ok(())
    }

    pub fn set_name(&self, id: DownloadId, name: &str) -> rusqlite::Result<()> {
        self.lock().execute(
            "UPDATE downloads SET file_name = ?2 WHERE id = ?1",
            params![id.0, name],
        )?;
        Ok(())
    }

    /// Records what the server says about the file, discarding any earlier progress.
    pub fn begin_file(
        &self,
        id: DownloadId,
        remote: &Remote,
        segments: &[Segment],
    ) -> rusqlite::Result<()> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE downloads SET size = ?2, etag = ?3, last_modified = ?4, ranged = ?5
             WHERE id = ?1",
            params![
                id.0,
                remote.size.map(|size| size as i64),
                remote.etag,
                remote.last_modified,
                remote.ranged
            ],
        )?;
        replace_segments(&transaction, id, segments)?;
        transaction.commit()
    }

    pub fn save_segments(&self, id: DownloadId, segments: &[Segment]) -> rusqlite::Result<()> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        replace_segments(&transaction, id, segments)?;
        transaction.commit()
    }

    pub fn set_completed(&self, id: DownloadId, path: &Path, size: u64) -> rusqlite::Result<()> {
        self.lock().execute(
            "UPDATE downloads SET state = ?2, error = NULL, final_path = ?3, size = ?4
             WHERE id = ?1",
            params![
                id.0,
                State::Completed.as_str(),
                path.to_string_lossy(),
                size as i64
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: DownloadId) -> rusqlite::Result<()> {
        self.lock()
            .execute("DELETE FROM downloads WHERE id = ?1", params![id.0])?;
        Ok(())
    }
}

fn replace_segments(
    connection: &Connection,
    id: DownloadId,
    segments: &[Segment],
) -> rusqlite::Result<()> {
    connection.execute("DELETE FROM segments WHERE download_id = ?1", params![id.0])?;
    let mut insert = connection.prepare(
        "INSERT INTO segments (download_id, seg_start, seg_end, written)
         VALUES (?1, ?2, ?3, ?4)",
    )?;
    for segment in segments {
        insert.execute(params![
            id.0,
            segment.start as i64,
            segment.end as i64,
            segment.written as i64
        ])?;
    }
    Ok(())
}
