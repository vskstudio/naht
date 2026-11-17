use rusqlite::{Connection, Transaction, TransactionBehavior};

use super::{SCHEMA_VERSION, SCHEMA_VERSION_KEY, StateError, read_meta_i64};

const SCHEMA_TABLE_COUNT: i64 = 4;

pub(super) fn schema_is_current(conn: &Connection) -> Result<bool, StateError> {
    let tables: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type = 'table' AND name IN ('meta', 'instances', 'assets', 'studio_sides')",
        [],
        |row| row.get(0),
    )?;
    if tables != SCHEMA_TABLE_COUNT {
        return Ok(false);
    }
    let legacy_assets: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM assets WHERE content_hash NOT LIKE 'blake3:%')",
        [],
        |row| row.get(0),
    )?;
    Ok(!legacy_assets && read_meta_i64(conn, SCHEMA_VERSION_KEY)? == Some(SCHEMA_VERSION))
}

pub(super) fn migrate(conn: &mut Connection) -> Result<(), StateError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
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
         );
         CREATE TABLE IF NOT EXISTS assets (
             content_hash TEXT PRIMARY KEY,
             asset_id     TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS studio_sides (
             path    TEXT PRIMARY KEY,
             class   TEXT NOT NULL,
             content TEXT
         );
         DELETE FROM assets WHERE content_hash NOT LIKE 'blake3:%';",
    )?;
    write_schema_version(&tx)?;
    tx.commit()?;
    Ok(())
}

fn write_schema_version(tx: &Transaction<'_>) -> Result<(), StateError> {
    match read_meta_i64(tx, SCHEMA_VERSION_KEY)? {
        Some(found) if found != SCHEMA_VERSION => Err(StateError::SchemaVersionMismatch {
            found,
            expected: SCHEMA_VERSION,
        }),
        Some(_) => Ok(()),
        None => {
            tx.execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2)",
                (SCHEMA_VERSION_KEY, SCHEMA_VERSION.to_string()),
            )?;
            Ok(())
        }
    }
}
