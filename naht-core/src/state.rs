//! The persisted last-sync base (architecture §5) — the thing neither Rojo nor Argon keeps.
//!
//! Lives in `.naht/state.db` (SQLite). Per instance, keyed by stable GUID, it records the path,
//! class, content hash, the last-synced `base_content` (the base for 3-way merge; text only), the
//! filesystem mtime, and whether the path is frozen pending conflict resolution. A `meta` table
//! holds the schema version and project identity. Writes are transactional and schema-versioned so
//! a restart re-diffs safely instead of re-clobbering.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension};

/// The on-disk schema version. Bumped only by a migration; an older or newer database is rejected
/// rather than silently misread.
const SCHEMA_VERSION: i64 = 1;

const SCHEMA_VERSION_KEY: &str = "schema_version";
const PROJECT_ID_KEY: &str = "project_id";

/// Errors from the state store.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    /// An underlying SQLite failure.
    #[error("state database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Could not create the directory holding the database.
    #[error("state I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The database was written by a different schema version.
    #[error("state schema version mismatch: found {found}, this build expects {expected}")]
    SchemaVersionMismatch {
        /// The version recorded in the database.
        found: i64,
        /// The version this build understands.
        expected: i64,
    },
    /// A `meta` value was not the expected shape.
    #[error("corrupt state metadata for {key:?}: {value:?}")]
    CorruptMeta {
        /// The metadata key.
        key: String,
        /// The unparseable value.
        value: String,
    },
}

/// One persisted instance base, keyed by stable GUID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceRecord {
    /// Stable instance identity across sessions.
    pub guid: String,
    /// The filesystem path this instance maps to.
    pub path: String,
    /// The Roblox class name.
    pub class: String,
    /// A fast change-detection hash of the last-synced content.
    pub content_hash: String,
    /// The last-synced content — the base for 3-way text merge. `None` for binary instances, which
    /// store only the hash.
    pub base_content: Option<Vec<u8>>,
    /// The filesystem modification time, as recorded at last sync.
    pub mtime: i64,
    /// Whether this path is frozen pending conflict resolution.
    pub conflicted: bool,
}

/// The SQLite-backed state store.
pub struct StateStore {
    conn: Connection,
}

impl StateStore {
    /// Open (creating if absent) the state database at `path`, running migrations and verifying the
    /// schema version.
    pub fn open(path: &Path) -> Result<Self, StateError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        Self::init(Connection::open(path)?)
    }

    /// Open an in-memory state database. Used by tests that do not need persistence.
    pub fn open_in_memory() -> Result<Self, StateError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, StateError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS instances (
                 guid         TEXT PRIMARY KEY,
                 path         TEXT NOT NULL,
                 class        TEXT NOT NULL,
                 content_hash TEXT NOT NULL,
                 base_content BLOB,
                 mtime        INTEGER NOT NULL,
                 conflicted   INTEGER NOT NULL DEFAULT 0
             );",
        )?;

        match read_meta_i64(&conn, SCHEMA_VERSION_KEY)? {
            Some(found) if found != SCHEMA_VERSION => {
                return Err(StateError::SchemaVersionMismatch {
                    found,
                    expected: SCHEMA_VERSION,
                });
            }
            Some(_) => {}
            None => {
                conn.execute(
                    "INSERT INTO meta (key, value) VALUES (?1, ?2)",
                    (SCHEMA_VERSION_KEY, SCHEMA_VERSION.to_string()),
                )?;
            }
        }

        Ok(Self { conn })
    }

    /// Record the project identity in `meta`, overwriting any prior value.
    pub fn set_project_id(&self, project_id: &str) -> Result<(), StateError> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            (PROJECT_ID_KEY, project_id),
        )?;
        Ok(())
    }

    /// The recorded project identity, if any.
    pub fn project_id(&self) -> Result<Option<String>, StateError> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                [PROJECT_ID_KEY],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Insert or replace the base for one instance.
    pub fn upsert(&self, record: &InstanceRecord) -> Result<(), StateError> {
        self.conn.execute(
            "INSERT INTO instances
                 (guid, path, class, content_hash, base_content, mtime, conflicted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(guid) DO UPDATE SET
                 path         = excluded.path,
                 class        = excluded.class,
                 content_hash = excluded.content_hash,
                 base_content = excluded.base_content,
                 mtime        = excluded.mtime,
                 conflicted   = excluded.conflicted",
            rusqlite::params![
                record.guid,
                record.path,
                record.class,
                record.content_hash,
                record.base_content,
                record.mtime,
                record.conflicted,
            ],
        )?;
        Ok(())
    }

    /// Fetch one instance base by GUID.
    pub fn get(&self, guid: &str) -> Result<Option<InstanceRecord>, StateError> {
        Ok(self
            .conn
            .query_row(
                "SELECT guid, path, class, content_hash, base_content, mtime, conflicted
                 FROM instances WHERE guid = ?1",
                [guid],
                row_to_record,
            )
            .optional()?)
    }

    /// Remove one instance base by GUID. Removing an absent GUID is a no-op.
    pub fn remove(&self, guid: &str) -> Result<(), StateError> {
        self.conn
            .execute("DELETE FROM instances WHERE guid = ?1", [guid])?;
        Ok(())
    }

    /// Mark or clear the `conflicted` flag for one instance.
    pub fn set_conflicted(&self, guid: &str, conflicted: bool) -> Result<(), StateError> {
        self.conn.execute(
            "UPDATE instances SET conflicted = ?2 WHERE guid = ?1",
            rusqlite::params![guid, conflicted],
        )?;
        Ok(())
    }

    /// Every persisted instance base, ordered by path. Used by the reconciler to find instances
    /// that exist in the base but no longer on a side (deletions).
    pub fn all(&self) -> Result<Vec<InstanceRecord>, StateError> {
        let mut stmt = self.conn.prepare(
            "SELECT guid, path, class, content_hash, base_content, mtime, conflicted
             FROM instances ORDER BY path",
        )?;
        let rows = stmt.query_map([], row_to_record)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Every instance currently frozen as conflicted, ordered by path.
    pub fn conflicted(&self) -> Result<Vec<InstanceRecord>, StateError> {
        let mut stmt = self.conn.prepare(
            "SELECT guid, path, class, content_hash, base_content, mtime, conflicted
             FROM instances WHERE conflicted = 1 ORDER BY path",
        )?;
        let rows = stmt.query_map([], row_to_record)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

fn row_to_record(row: &rusqlite::Row) -> rusqlite::Result<InstanceRecord> {
    Ok(InstanceRecord {
        guid: row.get(0)?,
        path: row.get(1)?,
        class: row.get(2)?,
        content_hash: row.get(3)?,
        base_content: row.get(4)?,
        mtime: row.get(5)?,
        conflicted: row.get(6)?,
    })
}

fn read_meta_i64(conn: &Connection, key: &str) -> Result<Option<i64>, StateError> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()?;
    match raw {
        None => Ok(None),
        Some(text) => text
            .parse::<i64>()
            .map(Some)
            .map_err(|_| StateError::CorruptMeta {
                key: key.to_string(),
                value: text,
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_record(guid: &str) -> InstanceRecord {
        InstanceRecord {
            guid: guid.to_string(),
            path: "src/Greeter.luau".to_string(),
            class: "ModuleScript".to_string(),
            content_hash: "abc123".to_string(),
            base_content: Some(b"return 1".to_vec()),
            mtime: 1_700_000_000,
            conflicted: false,
        }
    }

    #[test]
    fn base_survives_a_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(".naht").join("state.db");
        let record = text_record("guid-1");
        {
            let store = StateStore::open(&db).unwrap();
            store.upsert(&record).unwrap();
        }
        let reopened = StateStore::open(&db).unwrap();
        assert_eq!(reopened.get("guid-1").unwrap(), Some(record));
    }

    #[test]
    fn upsert_overwrites_in_place() {
        let store = StateStore::open_in_memory().unwrap();
        store.upsert(&text_record("g")).unwrap();
        let mut updated = text_record("g");
        updated.content_hash = "def456".to_string();
        updated.base_content = Some(b"return 2".to_vec());
        store.upsert(&updated).unwrap();
        assert_eq!(store.get("g").unwrap(), Some(updated));
    }

    #[test]
    fn schema_version_mismatch_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("state.db");
        StateStore::open(&db).unwrap();
        // Simulate a database written by a different schema version.
        let raw = Connection::open(&db).unwrap();
        raw.execute(
            "UPDATE meta SET value = '999' WHERE key = 'schema_version'",
            [],
        )
        .unwrap();
        drop(raw);
        assert!(matches!(
            StateStore::open(&db),
            Err(StateError::SchemaVersionMismatch {
                found: 999,
                expected: 1
            })
        ));
    }

    #[test]
    fn conflicted_flag_round_trips_and_lists() {
        let store = StateStore::open_in_memory().unwrap();
        store.upsert(&text_record("g")).unwrap();
        assert!(store.conflicted().unwrap().is_empty());

        store.set_conflicted("g", true).unwrap();
        assert!(store.get("g").unwrap().unwrap().conflicted);
        assert_eq!(store.conflicted().unwrap().len(), 1);

        store.set_conflicted("g", false).unwrap();
        assert!(store.conflicted().unwrap().is_empty());
    }

    #[test]
    fn binary_instances_store_no_base_content() {
        let store = StateStore::open_in_memory().unwrap();
        let binary = InstanceRecord {
            guid: "mesh".to_string(),
            path: "Model.rbxm".to_string(),
            class: "Model".to_string(),
            content_hash: "hash-only".to_string(),
            base_content: None,
            mtime: 1,
            conflicted: false,
        };
        store.upsert(&binary).unwrap();
        assert_eq!(store.get("mesh").unwrap().unwrap().base_content, None);
    }

    #[test]
    fn remove_deletes_the_record() {
        let store = StateStore::open_in_memory().unwrap();
        store.upsert(&text_record("g")).unwrap();
        store.remove("g").unwrap();
        assert_eq!(store.get("g").unwrap(), None);
    }

    #[test]
    fn project_id_round_trips() {
        let store = StateStore::open_in_memory().unwrap();
        assert_eq!(store.project_id().unwrap(), None);
        store.set_project_id("proj-42").unwrap();
        assert_eq!(store.project_id().unwrap(), Some("proj-42".to_string()));
    }
}
