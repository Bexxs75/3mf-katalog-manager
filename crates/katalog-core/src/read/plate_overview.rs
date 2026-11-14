use super::geometry::resolve_within_roots;
use super::*;
use crate::threemf::plate_overview as plates;
use std::path::{Path, PathBuf};

/// Objects and preview pictures per plate, read from the file itself. A file that is not a 3MF, or
/// holds no plate list, gives no rows; a file that cannot be opened is an error with a code, so the
/// dialog can say "file not found" and keep the name, time and weight it already has.
/// `allowed_roots` works as in [`get_model_geometry`](super::get_model_geometry).
pub fn read_plate_overview(
    path: &Path,
    file_type: db::models::FileType,
    allowed_roots: Option<&[PathBuf]>,
    with_previews: bool,
) -> ReadResult<PlateOverviewDto> {
    if file_type != db::models::FileType::ThreeMf {
        return Ok(PlateOverviewDto::default());
    }
    let target = resolve_within_roots(path, allowed_roots)?;
    let file = std::fs::File::open(&target).map_err(geometry_io_error)?;
    let Ok(mut archive) = zip::ZipArchive::new(std::io::BufReader::new(file)) else {
        return Ok(PlateOverviewDto::default());
    };
    Ok(PlateOverviewDto {
        plates: plates::read_plate_overview(&mut archive, with_previews)
            .into_iter()
            .map(|row| PlateOverviewRowDto {
                plate_index: row.plate_index,
                object_count: row.object_count,
                preview: encode_image(row.preview),
            })
            .collect(),
    })
}
