//! Removal of catalog records (files, a folder subtree, the whole catalog). Database only:
//! files on disk are never touched. The SQL is the desktop app's; the web write API builds on
//! the same functions. No file-system access, so the module compiles on every platform.
use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension};

use crate::db::{self, error::DbError, FOLDER_SUBTREE_CTE};
use crate::error_code::ErrorCode;
use crate::import::file_names::SETTING_CATALOG_BASE_DIR;

/// What a folder removal would take along.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRemovalSummary {
    pub name: String,
    pub subfolder_count: i64,
    pub model_count: i64,
}

/// What a folder removal or a reset took away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogRemovalCount {
    pub model_count: i64,
    pub folder_count: i64,
}

/// The body of a files removal, for a caller that already holds a transaction: every id must be
/// an active (not trashed) model, otherwise nothing is removed. Returns the number of distinct ids.
pub fn remove_files_in(tx: &Connection, ids: &[i64]) -> Result<usize, DbError> {
    let ids: BTreeSet<i64> = ids.iter().copied().collect();
    for id in &ids {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM files WHERE id = ?1 AND deleted_at IS NULL)",
            [id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(DbError::coded(ErrorCode::ModelNotFoundOrTrashed));
        }
    }
    for id in &ids {
        tx.execute("DELETE FROM files WHERE id = ?1", [id])?;
    }
    db::bundles::prune_bundles(tx)?;
    Ok(ids.len())
}

/// Removes the given models from the catalog in one transaction.
pub fn remove_files(conn: &Connection, ids: &[i64]) -> Result<usize, DbError> {
    let tx = conn.unchecked_transaction()?;
    let removed = remove_files_in(&tx, ids)?;
    tx.commit()?;
    Ok(removed)
}

pub fn folder_summary(conn: &Connection, folder_id: i64) -> Result<FolderRemovalSummary, DbError> {
    let name = conn
        .query_row("SELECT name FROM folders WHERE id = ?1", [folder_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| DbError::coded(ErrorCode::FolderNotFound))?;
    let subfolder_count = conn.query_row(
        &format!("{FOLDER_SUBTREE_CTE} SELECT COUNT(*) - 1 FROM subtree"),
        [folder_id],
        |r| r.get(0),
    )?;
    let model_count = conn.query_row(
        &format!("{FOLDER_SUBTREE_CTE} SELECT COUNT(*) FROM files WHERE folder_id IN (SELECT id FROM subtree) AND deleted_at IS NULL"),
        [folder_id],
        |r| r.get(0),
    )?;
    Ok(FolderRemovalSummary { name, subfolder_count, model_count })
}

/// Removes a folder, its subfolders and their active models from the catalog. Refused when the
/// subtree contains the catalog base directory.
pub fn remove_folder(conn: &Connection, folder_id: i64) -> Result<CatalogRemovalCount, DbError> {
    let tx = conn.unchecked_transaction()?;
    let summary = folder_summary(&tx, folder_id)?;
    // Protect ancestors too: cascading their removal must not bypass the base-directory guard.
    let contains_base: bool = tx.query_row(
        &format!("{FOLDER_SUBTREE_CTE} SELECT EXISTS(SELECT 1 FROM folders WHERE id IN (SELECT id FROM subtree) AND path = (SELECT value FROM app_settings WHERE key = ?2))"),
        rusqlite::params![folder_id, SETTING_CATALOG_BASE_DIR],
        |r| r.get(0),
    )?;
    if contains_base {
        return Err(DbError::coded(ErrorCode::CatalogBaseDirProtected));
    }
    tx.execute(
        &format!("{FOLDER_SUBTREE_CTE} DELETE FROM files WHERE folder_id IN (SELECT id FROM subtree) AND deleted_at IS NULL"),
        [folder_id],
    )?;
    db::bundles::prune_bundles(&tx)?;
    tx.execute("DELETE FROM folders WHERE id = ?1", [folder_id])?;
    tx.commit()?;
    Ok(CatalogRemovalCount { model_count: summary.model_count, folder_count: summary.subfolder_count + 1 })
}

/// Empties the catalog: models, folders, tags, collections, bundles and the base directory setting.
pub fn reset(conn: &Connection) -> Result<CatalogRemovalCount, DbError> {
    let tx = conn.unchecked_transaction()?;
    let folder_count = tx.query_row("SELECT COUNT(*) FROM folders", [], |r| r.get(0))?;
    let model_count = tx.execute("DELETE FROM files WHERE deleted_at IS NULL", [])? as i64;
    // Reset detaches the retained job history from the catalog, including trash entries.
    tx.execute("UPDATE printer_jobs SET booked_file_id = NULL", [])?;
    tx.execute("DELETE FROM folders", [])?;
    tx.execute("DELETE FROM tags", [])?;
    tx.execute("DELETE FROM collections", [])?;
    db::bundles::clear_bundles(&tx)?;
    tx.execute("DELETE FROM app_settings WHERE key = ?1", [SETTING_CATALOG_BASE_DIR])?;
    tx.commit()?;
    Ok(CatalogRemovalCount { model_count, folder_count })
}
