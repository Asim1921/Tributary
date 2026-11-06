//! Saved downloads and their progress.
//!
//! Secret headers such as cookies must never reach this module. The engine removes
//! them first, and a test checks the database file for them.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::Error;
use crate::remote::Remote;
use crate::segments::Segment;

const SETTINGS: &str = "
    PRAGMA journal_mode = WAL;
    PRAGMA synchronous = NORMAL;
    PRAGMA foreign_keys = ON;
";

/// Each entry upgrades the database by one version, tracked in `PRAGMA user_version`.
/// Add new entries to the end. Never change an entry that has been released.
const MIGRATIONS: [&str; 2] = [
    "
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
    ",
    "
    ALTER TABLE downloads ADD COLUMN description  TEXT;
    ALTER TABLE downloads ADD COLUMN added_at     INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE downloads ADD COLUMN completed_at INTEGER;
    ALTER TABLE downloads ADD COLUMN part_dir     TEXT;
    ",
];

const SELECT_DOWNLOAD: &str = "
    SELECT d.id, d.url, d.dest_dir, d.file_name, d.headers, d.size, d.etag, d.last_modified,
           d.ranged, d.state, d.error, d.final_path,
           COALESCE((SELECT SUM(s.written) FROM segments s WHERE s.download_id = d.id), 0),
           d.description, d.added_at, d.completed_at, d.part_dir
    FROM downloads d
";

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

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
    pub description: Option<String>,
    /// When it was added, in seconds since 1970.
    pub added_at: i64,
    pub completed_at: Option<i64>,
    /// Where the `.part` file lives. Fixed when the download first runs, so later
    /// changes to the destination or the temporary folder can't lose it.
    pub part_dir: Option<PathBuf>,
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
            description: row.get(13)?,
            added_at: row.get(14)?,
            completed_at: row.get(15)?,
            part_dir: row.get::<_, Option<String>>(16)?.map(PathBuf::from),
        })
    }
}

pub struct Store {
    connection: Mutex<Connection>,
}

impl Store {
    /// Opens the database. Anything left running belonged to a process that died,
    /// so it becomes paused and waits to be resumed.
    pub fn open(path: &Path) -> Result<Self, Error> {
        let mut connection = Connection::open(path)?;
        connection.execute_batch(SETTINGS)?;
        migrate(&mut connection)?;
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
        description: Option<&str>,
        headers: &[(String, String)],
    ) -> rusqlite::Result<DownloadId> {
        let headers = serde_json::to_string(headers).unwrap_or_else(|_| "[]".to_owned());
        let connection = self.lock();
        connection.execute(
            "INSERT INTO downloads (url, dest_dir, file_name, description, headers, state, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                url,
                dest_dir.to_string_lossy(),
                file_name,
                description,
                headers,
                State::Paused.as_str(),
                now()
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

    pub fn set_part_dir(&self, id: DownloadId, part_dir: &Path) -> rusqlite::Result<()> {
        self.lock().execute(
            "UPDATE downloads SET part_dir = ?2 WHERE id = ?1",
            params![id.0, part_dir.to_string_lossy()],
        )?;
        Ok(())
    }

    /// Changes the given fields and leaves the others as they are.
    pub fn edit(
        &self,
        id: DownloadId,
        url: Option<&str>,
        dest_dir: Option<&Path>,
        description: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.lock().execute(
            "UPDATE downloads
             SET url = COALESCE(?2, url),
                 dest_dir = COALESCE(?3, dest_dir),
                 description = COALESCE(?4, description)
             WHERE id = ?1",
            params![
                id.0,
                url,
                dest_dir.map(|path| path.to_string_lossy()),
                description
            ],
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
            "UPDATE downloads
             SET state = ?2, error = NULL, final_path = ?3, size = ?4, completed_at = ?5
             WHERE id = ?1",
            params![
                id.0,
                State::Completed.as_str(),
                path.to_string_lossy(),
                size as i64,
                now()
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

/// Brings the database up to the current version, one committed step at a time.
fn migrate(connection: &mut Connection) -> Result<(), Error> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > MIGRATIONS.len() as i64 {
        return Err(Error::NewerDatabase);
    }

    for (index, migration) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let transaction = connection.transaction()?;
        transaction.execute_batch(migration)?;
        transaction.pragma_update(None, "user_version", index as i64 + 1)?;
        transaction.commit()?;
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn version_of(path: &Path) -> i64 {
        Connection::open(path)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap()
    }

    const LATEST: i64 = MIGRATIONS.len() as i64;

    #[test]
    fn a_new_database_reaches_the_latest_version() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.db");

        Store::open(&path).unwrap();

        assert_eq!(version_of(&path), LATEST);
    }

    #[test]
    fn upgrading_keeps_existing_downloads() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.db");

        let old = Connection::open(&path).unwrap();
        old.execute_batch(MIGRATIONS[0]).unwrap();
        old.execute(
            "INSERT INTO downloads (url, dest_dir, headers, state) VALUES ('u', 'd', '[]', 'paused')",
            [],
        )
        .unwrap();
        drop(old);

        let store = Store::open(&path).unwrap();
        let downloads = store.list().unwrap();

        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].url, "u");
        assert_eq!(downloads[0].description, None);
        assert_eq!(version_of(&path), LATEST);
    }

    #[test]
    fn opening_twice_changes_nothing() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.db");

        let id = Store::open(&path)
            .unwrap()
            .insert("u", Path::new("d"), None, Some("note"), &[])
            .unwrap();

        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.get(id).unwrap().unwrap().description.as_deref(),
            Some("note")
        );
    }

    #[test]
    fn refuses_a_database_from_a_newer_version() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.db");

        let connection = Connection::open(&path).unwrap();
        connection
            .pragma_update(None, "user_version", LATEST + 1)
            .unwrap();
        drop(connection);

        assert!(matches!(Store::open(&path), Err(Error::NewerDatabase)));
    }

    #[test]
    fn editing_changes_only_the_given_fields() {
        let folder = tempfile::tempdir().unwrap();
        let store = Store::open(&folder.path().join("a.db")).unwrap();
        let id = store
            .insert("old", Path::new("dir"), None, Some("note"), &[])
            .unwrap();

        store.edit(id, Some("new"), None, None).unwrap();

        let download = store.get(id).unwrap().unwrap();
        assert_eq!(download.url, "new");
        assert_eq!(download.dest_dir, PathBuf::from("dir"));
        assert_eq!(download.description.as_deref(), Some("note"));
    }
}
