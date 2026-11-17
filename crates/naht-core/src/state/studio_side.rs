use rusqlite::OptionalExtension;

use super::{StateError, StateStore};

/// The latest Studio side of a conflicted text path, kept so a restart seeds the mirror with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudioSide {
    pub(crate) path: String,
    pub(crate) class: String,
    pub(crate) content: Option<String>,
}

impl StateStore {
    /// Record the latest Studio side of a path, or `None` when Studio deleted it.
    ///
    /// # Errors
    ///
    /// Returns an error when the Studio side cannot be written.
    pub fn set_studio_side(
        &self,
        path: &str,
        class: &str,
        content: Option<&str>,
    ) -> Result<(), StateError> {
        self.conn.execute(
            "INSERT INTO studio_sides (path, class, content) VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET
                 class   = excluded.class,
                 content = excluded.content",
            (path, class, content),
        )?;
        Ok(())
    }

    /// The recorded Studio side of a path, if one is tracked.
    ///
    /// # Errors
    ///
    /// Returns an error when the Studio side cannot be read.
    pub fn studio_side(&self, path: &str) -> Result<Option<StudioSide>, StateError> {
        Ok(self
            .conn
            .query_row(
                "SELECT path, class, content FROM studio_sides WHERE path = ?1",
                [path],
                row_to_studio_side,
            )
            .optional()?)
    }

    /// Every tracked Studio side, ordered by path.
    ///
    /// # Errors
    ///
    /// Returns an error when the Studio sides cannot be read.
    pub fn studio_sides(&self) -> Result<Vec<StudioSide>, StateError> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, class, content FROM studio_sides ORDER BY path")?;
        let rows = stmt.query_map([], row_to_studio_side)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Stop tracking the Studio side of a path, once its base has caught up with Studio.
    ///
    /// # Errors
    ///
    /// Returns an error when the Studio side cannot be cleared.
    pub fn clear_studio_side(&self, path: &str) -> Result<(), StateError> {
        self.conn
            .execute("DELETE FROM studio_sides WHERE path = ?1", [path])?;
        Ok(())
    }
}

fn row_to_studio_side(row: &rusqlite::Row<'_>) -> rusqlite::Result<StudioSide> {
    Ok(StudioSide {
        path: row.get(0)?,
        class: row.get(1)?,
        content: row.get(2)?,
    })
}
