//! Read-side services. No initialization, cleanup, background jobs or writes.
mod dto;
mod error;
mod geometry;
pub use dto::*;
pub use error::{GeometryErrorCode, ReadError};
pub use geometry::{
    encode_render_meshes, geometry_io_error, get_model_geometry, get_model_geometry_with_limits, load_geometry_source,
    GeometrySource,
};
pub type ReadResult<T> = Result<T, ReadError>;
use crate::{db, threemf};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;

/// Slim projection of `ModelFileDto` for grid and list: without image blobs
/// and `tags`, which only the detail page loads via
/// `list_files_by_ids([id])`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSummaryDto {
    pub license: Option<String>,
    pub materials: Vec<MaterialDto>,
    pub estimated_weight_g: Option<f64>,
    pub weight_source: String,
    pub id: String,
    pub name: String,
    pub path: String,
    pub file_type: String,
    // Empty string = no folder, as in `ModelFileDto.folder_id` and the frontend.
    pub folder_id: String,
    pub file_size_bytes: i64,
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub object_count: Option<i64>,
    pub imported_at: String,
    pub file_modified_at: Option<String>,
    pub print_status: String,
    pub favorite: bool,
    pub queue_position: Option<i64>,
    pub has_thumbnail: bool,
    pub has_render_snapshot: bool,
    pub creator: Option<String>,
    pub last_viewed_at: Option<String>,
    pub content_hash: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCountDto {
    pub label: String,
    pub count: i64,
    pub color_hue: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileImagesDto {
    pub id: String,
    pub thumbnail_image: Option<String>,
    pub render_snapshot_image: Option<String>,
    pub custom_image: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintLogEntryDto {
    pub id: String,
    pub printed_at: String,
    pub note: Option<String>,
    pub photo_image: Option<String>,
}

pub fn summary_to_dto(s: db::FileSummary) -> FileSummaryDto {
    let slice_info = s
        .slice_info_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<threemf::SliceInfo>(json).ok());
    let (estimated_weight_g, weight_source) = match slice_info {
        Some(info) => (Some(info.total_weight_g), "slicer"),
        None => (
            estimate_weight_g(s.volume_cm3, s.materials.first().map(|m| m.name.as_str())),
            "estimated",
        ),
    };
    FileSummaryDto {
        license: s.license,
        estimated_weight_g,
        weight_source: weight_source.into(),
        materials: s
            .materials
            .into_iter()
            .map(|m| MaterialDto {
                name: m.name,
                display_color: m.display_color,
            })
            .collect(),
        id: s.id.to_string(),
        name: s.name,
        path: s.path,
        file_type: s.file_type.as_str().to_string(),
        folder_id: s.folder_id.map(|id| id.to_string()).unwrap_or_default(),
        file_size_bytes: s.file_size_bytes,
        dimensions_mm: s.dimensions_mm,
        volume_cm3: s.volume_cm3,
        object_count: s.object_count,
        imported_at: s.imported_at,
        file_modified_at: s.file_modified_at,
        print_status: s.print_status,
        favorite: s.favorite,
        queue_position: s.queue_position,
        has_thumbnail: s.has_thumbnail,
        has_render_snapshot: s.has_render_snapshot,
        creator: s.creator,
        last_viewed_at: s.last_viewed_at,
        content_hash: s.content_hash,
    }
}

pub fn print_log_entry_to_dto(record: db::models::PrintLogEntryRecord) -> PrintLogEntryDto {
    PrintLogEntryDto {
        id: record.id.to_string(),
        printed_at: record.printed_at,
        note: record.note,
        photo_image: encode_image(record.photo_png),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderDto {
    pub id: String,
    pub name: String,
    pub path: String,
    pub parent_id: Option<String>,
    pub count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDto {
    pub id: String,
    pub name: String,
    pub model_count: i64,
}

fn to_collection_dto(record: db::models::CollectionRecord) -> CollectionDto {
    CollectionDto {
        id: record.id.to_string(),
        name: record.name,
        model_count: record.model_count,
    }
}

pub fn list_file_summaries(conn: &Connection) -> ReadResult<Vec<FileSummaryDto>> {
    let summaries = db::list_file_summaries(conn).map_err(ReadError::from)?;
    Ok(summaries.into_iter().map(summary_to_dto).collect())
}

pub fn list_all_file_tags(conn: &Connection) -> ReadResult<HashMap<String, Vec<String>>> {
    let pairs = db::list_all_file_tags(conn).map_err(ReadError::from)?;
    let mut by_file: HashMap<String, Vec<String>> = HashMap::new();
    for (file_id, tag) in pairs {
        by_file.entry(file_id.to_string()).or_default().push(tag);
    }
    Ok(by_file)
}

pub fn list_files_by_ids(conn: &Connection, ids: Vec<String>) -> ReadResult<Vec<ModelFileDto>> {
    let parsed_ids: Vec<i64> = ids
        .iter()
        .map(|id| id.parse().map_err(|_| format!("invalid file id: {id}")))
        .collect::<Result<_, String>>()?;
    let files = db::list_files_by_ids(conn, &parsed_ids).map_err(ReadError::from)?;
    let spools = db::list_filament_spools(conn).map_err(ReadError::from)?;
    Ok(files.into_iter().map(|f| to_dto(f, &spools)).collect())
}

pub fn list_tag_counts(conn: &Connection) -> ReadResult<Vec<TagCountDto>> {
    let tags = db::list_tag_counts(conn).map_err(ReadError::from)?;
    Ok(tags
        .into_iter()
        .map(|t| TagCountDto {
            label: t.name,
            count: t.count,
            color_hue: t.color_hue,
        })
        .collect())
}

pub fn list_print_log_entries(
    conn: &Connection,
    file_id: String,
) -> ReadResult<Vec<PrintLogEntryDto>> {
    let fid: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let entries = db::list_print_log_entries(conn, fid).map_err(ReadError::from)?;
    Ok(entries.into_iter().map(print_log_entry_to_dto).collect())
}

pub fn list_folders(conn: &Connection) -> ReadResult<Vec<FolderDto>> {
    let folders = db::list_folders(conn).map_err(ReadError::from)?;
    let files = db::list_files(conn).map_err(ReadError::from)?;

    fn is_descendant_or_self(
        folders: &[db::models::FolderRecord],
        candidate_id: i64,
        ancestor_id: i64,
    ) -> bool {
        if candidate_id == ancestor_id {
            return true;
        }
        let mut current = candidate_id;
        while let Some(f) = folders.iter().find(|f| f.id == current) {
            match f.parent_id {
                Some(pid) if pid == ancestor_id => return true,
                Some(pid) => current = pid,
                None => return false,
            }
        }
        false
    }

    let dtos = folders
        .iter()
        .map(|folder| {
            let count = files
                .iter()
                .filter(|f| {
                    f.folder_id
                        .is_some_and(|fid| is_descendant_or_self(&folders, fid, folder.id))
                })
                .count() as i64;
            FolderDto {
                id: folder.id.to_string(),
                name: folder.name.clone(),
                path: folder.path.clone(),
                parent_id: folder.parent_id.map(|id| id.to_string()),
                count,
            }
        })
        .collect();

    Ok(dtos)
}

pub fn list_collections(conn: &Connection) -> ReadResult<Vec<CollectionDto>> {
    let collections = db::list_collections(conn).map_err(ReadError::from)?;
    Ok(collections.into_iter().map(to_collection_dto).collect())
}

pub fn list_collection_files(
    conn: &Connection,
    collection_id: String,
) -> ReadResult<Vec<ModelFileDto>> {
    let cid: i64 = collection_id
        .parse()
        .map_err(|_| "invalid collection id".to_string())?;
    let ids = db::list_collection_file_ids(conn, cid).map_err(ReadError::from)?;
    let files = db::list_files_by_ids(conn, &ids).map_err(ReadError::from)?;
    let spools = db::list_filament_spools(conn).map_err(ReadError::from)?;
    Ok(files.into_iter().map(|f| to_dto(f, &spools)).collect())
}

pub fn list_trash(conn: &Connection) -> ReadResult<Vec<ModelFileDto>> {
    let files = db::list_trash(conn).map_err(ReadError::from)?;
    let spools = db::list_filament_spools(conn).map_err(ReadError::from)?;
    Ok(files.into_iter().map(|f| to_dto(f, &spools)).collect())
}

pub fn load_file_images(conn: &Connection, ids: &[i64]) -> ReadResult<Vec<db::FileImages>> {
    db::list_file_images(conn, ids).map_err(ReadError::from)
}

pub fn encode_file_images(images: Vec<db::FileImages>) -> Vec<FileImagesDto> {
    images
        .into_iter()
        .map(|images| FileImagesDto {
            id: images.id.to_string(),
            thumbnail_image: encode_image(images.thumbnail_png),
            render_snapshot_image: encode_image(images.render_snapshot_png),
            custom_image: encode_image(images.custom_image_png),
        })
        .collect()
}

#[cfg(test)]
mod tests;

pub mod manager;
