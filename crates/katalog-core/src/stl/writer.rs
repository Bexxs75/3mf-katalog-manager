//! Binary STL writer for the render mesh list (`Vec<RenderMesh>`), the same
//! list the preview shows. Positions are written as they are in the list
//! (3MF: already in world coordinates, build transforms applied; unit
//! mm, no unit conversion). STL knows no colors, so they are dropped.
//!
//! Layout: 80 byte header, u32 triangle count, then 50 bytes per triangle
//! (normal, 3 x vertex as f32 little endian, 2 attribute bytes = 0).

use std::fmt;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::geometry::RenderMesh;

const HEADER_LEN: usize = 80;
const FACET_LEN: usize = 50;
/// Triangles per write call (400 KB); the only copy of mesh data besides the output.
pub(super) const BLOCK_TRIANGLES: usize = 8192;
/// The facet count of binary STL is a `u32`.
pub const MAX_STL_TRIANGLES: u64 = u32::MAX as u64;

/// Which part of the mesh list is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StlScope<'a> {
    /// Every mesh (for 3MF with several plates: all plates).
    All,
    /// Only meshes of this plate number.
    Plate(u32),
    /// Only these mesh indices (the list is applied in mesh order, duplicates
    /// ignored, so the result does not depend on the click order).
    Parts(&'a [usize]),
}

#[derive(Debug)]
pub enum StlWriteError {
    /// The selection contains no triangle.
    NothingToExport,
    /// A part index is outside of the mesh list.
    PartOutOfRange { index: usize, count: usize },
    /// A triangle references a vertex that does not exist.
    InvalidIndex,
    /// A coordinate is NaN or infinite.
    NonFinite,
    /// More triangles than the limit (binary STL: `u32::MAX`).
    TooManyTriangles { count: u64, max: u64 },
    /// The target is the source file itself.
    TargetIsSource,
    /// The source file changed after it was read.
    SourceChanged,
    Io(io::Error),
}

impl fmt::Display for StlWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StlWriteError::NothingToExport => write!(f, "nothing to export: the selection has no triangles"),
            StlWriteError::PartOutOfRange { index, count } => {
                write!(f, "part {index} does not exist (the model has {count} parts)")
            }
            StlWriteError::InvalidIndex => write!(f, "a triangle references a missing vertex"),
            StlWriteError::NonFinite => write!(f, "a coordinate is not a finite number"),
            StlWriteError::TooManyTriangles { count, max } => {
                write!(f, "too many triangles for binary STL: {count} (maximum {max})")
            }
            StlWriteError::TargetIsSource => write!(f, "the target is the source file"),
            StlWriteError::SourceChanged => write!(f, "the source file changed"),
            StlWriteError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for StlWriteError {}

impl From<io::Error> for StlWriteError {
    fn from(e: io::Error) -> Self {
        StlWriteError::Io(e)
    }
}

/// Picks the meshes for `scope`. Meshes without triangles stay out of the result.
pub fn select_meshes<'a>(meshes: &'a [RenderMesh], scope: StlScope<'_>) -> Result<Vec<&'a RenderMesh>, StlWriteError> {
    let selected: Vec<&RenderMesh> = match scope {
        StlScope::All => meshes.iter().collect(),
        StlScope::Plate(number) => meshes.iter().filter(|m| m.plate == Some(number)).collect(),
        StlScope::Parts(indices) => {
            if let Some(&index) = indices.iter().find(|&&i| i >= meshes.len()) {
                return Err(StlWriteError::PartOutOfRange { index, count: meshes.len() });
            }
            let mut wanted = vec![false; meshes.len()];
            for &i in indices {
                wanted[i] = true;
            }
            meshes.iter().zip(wanted).filter_map(|(m, w)| w.then_some(m)).collect()
        }
    };
    let selected: Vec<&RenderMesh> = selected.into_iter().filter(|m| !m.indices.is_empty()).collect();
    if selected.is_empty() {
        return Err(StlWriteError::NothingToExport);
    }
    Ok(selected)
}

/// Writes `meshes` as binary STL to `out` and returns the triangle count.
pub fn write_stl<W: Write>(out: &mut W, meshes: &[&RenderMesh]) -> Result<u64, StlWriteError> {
    write_stl_with_limit(out, meshes, MAX_STL_TRIANGLES)
}

/// Like [`write_stl`] with a configurable triangle limit (for tests of the limit).
/// Everything is validated before the first byte is written, so an error never
/// leaves a half-written stream behind.
pub fn write_stl_with_limit<W: Write>(out: &mut W, meshes: &[&RenderMesh], max_triangles: u64) -> Result<u64, StlWriteError> {
    let count = validate(meshes, max_triangles)?;

    let mut header = [0u8; HEADER_LEN];
    // Must not start with "solid": some readers would take the file for ASCII STL.
    let label = b"Binary STL export";
    header[..label.len()].copy_from_slice(label);
    out.write_all(&header)?;
    out.write_all(&(count as u32).to_le_bytes())?;

    let mut block = Vec::with_capacity(BLOCK_TRIANGLES * FACET_LEN);
    for mesh in meshes {
        for triangle in &mesh.indices {
            let [a, b, c] = corners(mesh, triangle);
            block.extend_from_slice(&facet_normal(a, b, c).map(f32::to_le_bytes).concat());
            for corner in [a, b, c] {
                block.extend_from_slice(&corner.map(f32::to_le_bytes).concat());
            }
            block.extend_from_slice(&[0, 0]);
            if block.len() >= BLOCK_TRIANGLES * FACET_LEN {
                out.write_all(&block)?;
                block.clear();
            }
        }
    }
    out.write_all(&block)?;
    Ok(count)
}

fn corners(mesh: &RenderMesh, triangle: &[u32; 3]) -> [[f32; 3]; 3] {
    triangle.map(|i| mesh.positions[i as usize])
}

/// Unit normal by the right-hand rule over the vertex order (what STL readers
/// use for orientation). Computed from the triangle, not from `RenderMesh.normals`
/// (`None` for 3MF). A triangle without area gets (0, 0, 0), which STL allows.
fn facet_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let p = |v: [f32; 3]| v.map(f64::from);
    let (a, b, c) = (p(a), p(b), p(c));
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 0.0 && len.is_finite() {
        [(n[0] / len) as f32, (n[1] / len) as f32, (n[2] / len) as f32]
    } else {
        [0.0; 3]
    }
}

/// Checks indices, coordinates and the limit; returns the triangle count.
fn validate(meshes: &[&RenderMesh], max_triangles: u64) -> Result<u64, StlWriteError> {
    let mut count: u64 = 0;
    for mesh in meshes {
        if mesh.positions.iter().any(|p| p.iter().any(|c| !c.is_finite())) {
            return Err(StlWriteError::NonFinite);
        }
        let vertices = mesh.positions.len();
        if mesh.indices.iter().any(|t| t.iter().any(|&i| i as usize >= vertices)) {
            return Err(StlWriteError::InvalidIndex);
        }
        count += mesh.indices.len() as u64;
        if count > max_triangles {
            return Err(StlWriteError::TooManyTriangles { count, max: max_triangles });
        }
    }
    if count == 0 {
        return Err(StlWriteError::NothingToExport);
    }
    Ok(count)
}

/// Size and modification time of the source file, taken when it was read, so
/// an export can refuse to continue when the file was replaced in the meantime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceStamp {
    len: u64,
    modified: Option<SystemTime>,
}

impl SourceStamp {
    pub fn of(path: &Path) -> io::Result<Self> {
        let meta = std::fs::metadata(path)?;
        Ok(SourceStamp { len: meta.len(), modified: meta.modified().ok() })
    }

    pub fn ensure_unchanged(&self, path: &Path) -> Result<(), StlWriteError> {
        if SourceStamp::of(path)? == *self {
            Ok(())
        } else {
            Err(StlWriteError::SourceChanged)
        }
    }
}

/// Writes the selection to `target` through a temporary file in the same
/// folder and renames it into place, so a failure leaves no partial file and
/// an existing target stays untouched. `source` is the file the meshes came
/// from; it is never overwritten. The target keeps the default permissions
/// of the user (no `harden_permissions`, this is a user-chosen file).
pub fn write_stl_file(target: &Path, source: Option<&Path>, meshes: &[&RenderMesh]) -> Result<u64, StlWriteError> {
    write_stl_file_with_limit(target, source, meshes, MAX_STL_TRIANGLES)
}

pub fn write_stl_file_with_limit(
    target: &Path,
    source: Option<&Path>,
    meshes: &[&RenderMesh],
    max_triangles: u64,
) -> Result<u64, StlWriteError> {
    if let Some(source) = source {
        if is_same_file(target, source) {
            return Err(StlWriteError::TargetIsSource);
        }
    }
    // Validate before touching the folder: an invalid selection creates no file.
    validate(meshes, max_triangles)?;
    let mut written = 0;
    replace_file_atomically(target, |file| {
        let mut buffered = io::BufWriter::new(file);
        written = write_stl_with_limit(&mut buffered, meshes, max_triangles).map_err(|e| match e {
            StlWriteError::Io(e) => e,
            other => io::Error::other(other.to_string()),
        })?;
        buffered.flush()
    })?;
    Ok(written)
}

/// Also true when both names reach the same file (symlink, `..`, hard link).
fn is_same_file(target: &Path, source: &Path) -> bool {
    let Ok(source_real) = source.canonicalize() else {
        return target == source;
    };
    let target_real = target.canonicalize().ok().or_else(|| {
        let name = target.file_name()?;
        let parent = target.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        Some(parent.canonicalize().ok()?.join(name))
    });
    if target_real.as_deref() == Some(source_real.as_path()) {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(t), Ok(s)) = (std::fs::metadata(target), std::fs::metadata(&source_real)) {
            return t.dev() == s.dev() && t.ino() == s.ino();
        }
    }
    false
}

/// Runs `write` on a new temporary file next to `target`, then renames it to
/// `target`. On any error the temporary file is removed and `target` is unchanged.
pub(crate) fn replace_file_atomically(target: &Path, write: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<()> {
    let dir = target.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let (temp, mut file) = create_temp_beside(dir)?;
    let result = write(&mut file).and_then(|()| file.sync_all());
    drop(file);
    let result = result.and_then(|()| std::fs::rename(&temp, target));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// Exclusive temp file (never follows a planted symlink) with the default
/// permissions of the user: the OS applies the umask to the 0o666 request.
fn create_temp_beside(dir: &Path) -> io::Result<(PathBuf, File)> {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let mut last_err = None;
    for attempt in 0..16 {
        let candidate = dir.join(format!(".stl-export.{}-{nanos}-{attempt}.tmp", std::process::id()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o666);
        }
        match options.open(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| io::Error::other("no temp file could be created")))
}
