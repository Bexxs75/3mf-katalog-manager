use super::*;
use crate::{geometry::RenderMesh, obj, stl};
use std::path::{Path, PathBuf};
/// Loaded while holding the database lock. `path` honors the trash location.
#[derive(Debug)]
pub struct GeometrySource {
    pub path: PathBuf,
    pub file_type: db::models::FileType,
}
pub fn load_geometry_source(conn: &Connection, file_id: &str) -> ReadResult<GeometrySource> {
    let id: i64 = file_id.parse().map_err(|_| "invalid file id".to_string())?;
    let file = db::get_file(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            ReadError::expected("file not found").with_code(GeometryErrorCode::NotFound)
        })?;
    Ok(GeometrySource {
        path: PathBuf::from(file.current_path()),
        file_type: file.file_type,
    })
}

/// Reads and encodes after releasing the database lock. `None` preserves the
/// desktop behavior. With roots, both roots and the target are canonicalized;
/// component-wise containment rejects traversal and symlinks leaving a root.
/// With roots the resolved path that passed the check is the one that gets opened.
/// This is still a path admission check, not protection against a directory in the
/// path being replaced concurrently by a filesystem writer; server shares must not
/// be attacker-writable.
pub fn get_model_geometry(path: &Path, allowed_roots: Option<&[PathBuf]>) -> ReadResult<Vec<u8>> {
    let mut target = path.to_path_buf();
    if let Some(roots) = allowed_roots {
        let canonical = path.canonicalize().map_err(geometry_io_error)?;
        let mut allowed = false;
        for root in roots {
            let root = root.canonicalize().map_err(geometry_io_error)?;
            if canonical.starts_with(root) {
                allowed = true;
            }
        }
        if !allowed {
            return Err(ReadError::expected(
                "Dateipfad liegt außerhalb der freigegebenen Verzeichnisse",
            )
            .with_code(GeometryErrorCode::Unreadable));
        }
        // Open exactly the resolved path that passed the check, so a symlink swapped
        // in afterwards cannot redirect the read.
        target = canonical;
    }
    Ok(encode_render_meshes(&load_geometry(&target)?))
}

pub fn encode_render_meshes(meshes: &[RenderMesh]) -> Vec<u8> {
    let mut palette: Vec<crate::geometry::RenderColor> = Vec::new();
    let mut palette_indices = std::collections::HashMap::new();
    let headers: Vec<serde_json::Value> = meshes
        .iter()
        .map(|m| {
            let local_indices: Vec<usize> = m
                .palette
                .iter()
                .map(|color| {
                    *palette_indices.entry(color).or_insert_with(|| {
                        let index = palette.len();
                        palette.push(color.clone());
                        index
                    })
                })
                .collect();
            let mut header = serde_json::json!({
                "vertexCount": m.positions.len(), "hasNormal": m.normals.is_some(),
                "indexCount": m.indices.len() * 3, "plate": m.plate,
            });
            if let Some(name) = &m.object_name {
                header["objectName"] = name.clone().into();
            }
            if !m.groups.is_empty() {
                header["groups"] = serde_json::json!(m
                    .groups
                    .iter()
                    .map(|g| crate::geometry::RenderGroup {
                        start: g.start,
                        count: g.count,
                        color_index: g.color_index.map(|i| local_indices[i]),
                    })
                    .collect::<Vec<_>>());
            }
            header
        })
        .collect();
    let plates: std::collections::BTreeMap<_, _> = meshes
        .iter()
        .filter_map(|m| m.plate.map(|n| (n, m.plate_name.clone())))
        .collect();
    let mut header = serde_json::json!({ "meshes": headers, "palette": palette });
    if plates.len() >= 2 {
        header["plates"] = serde_json::json!(plates
            .into_iter()
            .map(|(number, name)| serde_json::json!({"number": number, "name": name}))
            .collect::<Vec<_>>());
    }
    let mut header_json =
        serde_json::to_vec(&header).expect("mesh header serialization cannot fail");
    while !(4 + header_json.len()).is_multiple_of(4) {
        header_json.push(b' ');
    }

    let payload: usize = meshes
        .iter()
        .map(|m| {
            m.positions.len() * 12
                + m.normals.as_ref().map_or(0, |n| n.len() * 12)
                + m.indices.len() * 12
        })
        .sum();
    let mut out = Vec::with_capacity(4 + header_json.len() + payload);
    out.extend_from_slice(&(header_json.len() as u32).to_le_bytes());
    out.extend_from_slice(&header_json);

    for mesh in meshes {
        for p in &mesh.positions {
            for &c in p {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        if let Some(normals) = &mesh.normals {
            for n in normals {
                for &c in n {
                    out.extend_from_slice(&c.to_le_bytes());
                }
            }
        }
        for tri in &mesh.indices {
            for &idx in tri {
                out.extend_from_slice(&idx.to_le_bytes());
            }
        }
    }

    out
}
pub fn geometry_io_error(e: std::io::Error) -> ReadError {
    use super::GeometryErrorCode as Code;
    let code = if e.kind() == std::io::ErrorKind::NotFound {
        Code::NotFound
    } else {
        Code::Unreadable
    };
    if matches!(code, Code::TooLarge | Code::NotFound) {
        ReadError::expected(e.to_string()).with_code(code)
    } else {
        ReadError::from(e.to_string()).with_code(code)
    }
}

fn geometry_3mf_error(e: threemf::ThreeMfError) -> ReadError {
    use super::GeometryErrorCode as Code;
    let code = match &e {
        threemf::ThreeMfError::EntryTooLarge { .. }
        | threemf::ThreeMfError::ResourceLimitExceeded(_)
        | threemf::ThreeMfError::MaxDepthExceeded => Code::TooLarge,
        threemf::ThreeMfError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => Code::NotFound,
        _ => Code::Unreadable,
    };
    if matches!(code, Code::TooLarge | Code::NotFound) {
        ReadError::expected(e.to_string()).with_code(code)
    } else {
        ReadError::from(e.to_string()).with_code(code)
    }
}

// Retains the existing safe-file budget and open-handle type check, while
// distinguishing its size failures without inspecting error text.
fn read_geometry_bytes(path: &Path, extension: &str) -> ReadResult<Vec<u8>> {
    use std::io::Read;
    let mut file = crate::safe_file::open_regular(path).map_err(geometry_io_error)?;
    check_preview_size(
        &mut file,
        extension,
        MAX_PREVIEW_TRIANGLES,
        MAX_ASCII_STL_PREVIEW_BYTES,
        MAX_OBJ_PREVIEW_BYTES,
    )?;
    let max = crate::safe_file::MAX_MODEL_FILE_BYTES;
    let too_large = || {
        ReadError::expected(format!("Datei überschreitet {max} Bytes"))
            .with_code(super::GeometryErrorCode::TooLarge)
    };
    if file.metadata().map_err(geometry_io_error)?.len() > max {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(geometry_io_error)?;
    if bytes.len() as u64 > max {
        return Err(too_large());
    }
    Ok(bytes)
}

// Same ceiling as the 3MF preview: beyond it the webview freezes or dies while
// the mesh is serialized and rendered, so the viewer shows its "too large" card.
const MAX_PREVIEW_TRIANGLES: u64 = 3_000_000;
// Text meshes need a cheap byte-size precheck because the whole text is held
// alongside the parsed mesh. OBJ additionally enforces the triangle budget
// while parsing; file size alone cannot bound repeated faces.
const MAX_ASCII_STL_PREVIEW_BYTES: u64 = 256 * 1024 * 1024;
const MAX_OBJ_PREVIEW_BYTES: u64 = 128 * 1024 * 1024;

fn preview_too_large(detail: String) -> ReadError {
    ReadError::expected(detail).with_code(super::GeometryErrorCode::TooLarge)
}

/// Rejects oversized files before reading them in full. OBJ triangle counts are
/// checked separately while parsing, since repeated faces can be very compact.
fn check_preview_size(
    file: &mut std::fs::File,
    extension: &str,
    max_triangles: u64,
    max_ascii_stl_bytes: u64,
    max_obj_bytes: u64,
) -> ReadResult<()> {
    use std::io::{Read, Seek, SeekFrom};
    let len = file.metadata().map_err(geometry_io_error)?.len();
    match extension {
        "stl" => {
            let mut head = [0u8; 84];
            let mut filled = 0;
            while filled < head.len() {
                match file.read(&mut head[filled..]).map_err(geometry_io_error)? {
                    0 => break,
                    n => filled += n,
                }
            }
            file.seek(SeekFrom::Start(0)).map_err(geometry_io_error)?;
            if filled == head.len() {
                let count = u64::from(u32::from_le_bytes([head[80], head[81], head[82], head[83]]));
                if len == 84 + 50 * count {
                    if count > max_triangles {
                        return Err(preview_too_large(format!(
                            "model too large for 3D preview ({count} triangles)"
                        )));
                    }
                    return Ok(());
                }
            }
            if len > max_ascii_stl_bytes {
                return Err(preview_too_large(format!(
                    "model too large for 3D preview ({len} bytes)"
                )));
            }
        }
        "obj" if len > max_obj_bytes => {
            return Err(preview_too_large(format!(
                "model too large for 3D preview ({len} bytes)"
            )));
        }
        _ => {}
    }
    Ok(())
}

pub fn load_geometry(path: &Path) -> ReadResult<Vec<RenderMesh>> {
    load_geometry_with_obj_limit(path, MAX_PREVIEW_TRIANGLES)
}

fn load_geometry_with_obj_limit(
    path: &Path,
    max_obj_triangles: u64,
) -> ReadResult<Vec<RenderMesh>> {
    use super::GeometryErrorCode as Code;
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let unreadable = |e: String| ReadError::from(e).with_code(Code::Unreadable);
    let meshes = match extension.as_str() {
        "stl" => vec![
            stl::parse_stl_geometry(&read_geometry_bytes(path, &extension)?)
                .map_err(|e| unreadable(e.to_string()))?,
        ],
        "obj" => vec![obj::parse_obj_geometry(
            &read_geometry_bytes(path, &extension)?,
            max_obj_triangles,
        )
        .map_err(|e| match e {
            obj::ObjError::TooLarge => preview_too_large(e.to_string()),
            _ => unreadable(e.to_string()),
        })?],
        "3mf" => threemf::extract_render_meshes_from_path(path).map_err(geometry_3mf_error)?,
        _ => {
            return Err(ReadError::expected(format!(
                "nicht unterstütztes Dateiformat: {extension}"
            ))
            .with_code(Code::Unsupported))
        }
    };
    if meshes
        .iter()
        .all(|m| m.indices.is_empty() || m.positions.is_empty())
    {
        return Err(unreadable("Keine darstellbare Geometrie".into()));
    }
    Ok(meshes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct HeaderEntryForTest {
        vertex_count: usize,
        has_normal: bool,
        index_count: usize,
    }
    type DecodedMesh = (Vec<[f32; 3]>, Option<Vec<[f32; 3]>>, Vec<u32>);

    fn decode_for_test(bytes: &[u8]) -> Vec<DecodedMesh> {
        let header_len = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        let header_json = std::str::from_utf8(&bytes[4..4 + header_len]).unwrap();
        let header: serde_json::Value = serde_json::from_str(header_json).unwrap();
        let headers: Vec<HeaderEntryForTest> =
            serde_json::from_value(header["meshes"].clone()).unwrap();

        let mut offset = 4 + header_len;
        let mut result = Vec::new();
        for h in headers {
            let mut positions = Vec::with_capacity(h.vertex_count);
            for _ in 0..h.vertex_count {
                let x = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                let y = f32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
                let z = f32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().unwrap());
                positions.push([x, y, z]);
                offset += 12;
            }

            let normals = if h.has_normal {
                let mut ns = Vec::with_capacity(h.vertex_count);
                for _ in 0..h.vertex_count {
                    let x = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                    let y = f32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
                    let z = f32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().unwrap());
                    ns.push([x, y, z]);
                    offset += 12;
                }
                Some(ns)
            } else {
                None
            };

            let mut indices = Vec::with_capacity(h.index_count);
            for _ in 0..h.index_count {
                let idx = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                indices.push(idx);
                offset += 4;
            }

            result.push((positions, normals, indices));
        }
        assert_eq!(
            offset,
            bytes.len(),
            "encoder should not leave trailing bytes"
        );
        result
    }
    #[test]
    fn encode_render_meshes_roundtrips_positions_normals_and_indices() {
        let meshes = vec![
            RenderMesh {
                positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                indices: vec![[0, 1, 2]],
                normals: Some(vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]]),
                ..Default::default()
            },
            RenderMesh {
                positions: vec![
                    [5.0, 5.0, 5.0],
                    [6.0, 5.0, 5.0],
                    [5.0, 6.0, 5.0],
                    [5.0, 5.0, 6.0],
                ],
                indices: vec![[0, 1, 2], [0, 1, 3]],
                normals: None,
                ..Default::default()
            },
        ];

        let bytes = encode_render_meshes(&meshes);
        let decoded = decode_for_test(&bytes);

        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].0, meshes[0].positions);
        assert_eq!(decoded[0].1, meshes[0].normals);
        assert_eq!(decoded[0].2, vec![0, 1, 2]);

        assert_eq!(decoded[1].0, meshes[1].positions);
        assert_eq!(decoded[1].1, None);
        assert_eq!(decoded[1].2, vec![0, 1, 2, 0, 1, 3]);
    }
    #[test]
    fn encode_render_meshes_roundtrips_material_groups_and_global_palette() {
        use crate::geometry::{RenderColor, RenderGroup};
        let red = RenderColor {
            name: "Rötlich".into(),
            color: "#ff0000".into(),
        };
        let blue = RenderColor {
            name: "Blue".into(),
            color: "#0000ff".into(),
        };
        let a = RenderMesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            indices: vec![[0, 1, 2], [0, 2, 1]],
            object_name: Some("Töpfchen".into()),
            groups: vec![
                RenderGroup {
                    start: 0,
                    count: 3,
                    color_index: None,
                },
                RenderGroup {
                    start: 3,
                    count: 3,
                    color_index: Some(0),
                },
            ],
            palette: vec![red.clone()],
            ..Default::default()
        };
        let b = RenderMesh {
            palette: vec![blue.clone(), red.clone()],
            groups: vec![
                RenderGroup {
                    start: 0,
                    count: 3,
                    color_index: Some(0),
                },
                RenderGroup {
                    start: 3,
                    count: 3,
                    color_index: Some(1),
                },
            ],
            ..a.clone()
        };
        let bytes = encode_render_meshes(&[a.clone(), b.clone()]);
        let decoded = decode_for_test(&bytes);
        assert_eq!(decoded[0].0, a.positions);
        assert_eq!(decoded[1].2, vec![0, 1, 2, 0, 2, 1]);
        let length = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        assert_eq!((4 + length) % 4, 0);
        let header: serde_json::Value = serde_json::from_slice(&bytes[4..4 + length]).unwrap();
        let palette: Vec<RenderColor> = serde_json::from_value(header["palette"].clone()).unwrap();
        assert_eq!(palette, vec![red, blue]);
        let groups: Vec<RenderGroup> =
            serde_json::from_value(header["meshes"][0]["groups"].clone()).unwrap();
        assert_eq!(groups, a.groups);
        let groups: Vec<RenderGroup> =
            serde_json::from_value(header["meshes"][1]["groups"].clone()).unwrap();
        assert_eq!(groups[0].color_index, Some(1));
        assert_eq!(groups[1].color_index, Some(0));
        assert_eq!(header["meshes"][0]["objectName"], "Töpfchen");
    }

    #[test]
    fn geometry_errors_keep_typed_codes_without_parsing_messages() {
        use crate::read::GeometryErrorCode as Code;
        for error in [
            threemf::ThreeMfError::MaxDepthExceeded,
            threemf::ThreeMfError::ResourceLimitExceeded("arbitrary text".into()),
            threemf::ThreeMfError::EntryTooLarge {
                path: "x".into(),
                size: 2,
                max: 1,
            },
        ] {
            assert_eq!(geometry_3mf_error(error).code, Some(Code::TooLarge));
        }
        assert_eq!(
            geometry_3mf_error(threemf::ThreeMfError::ComponentCycle).code,
            Some(Code::Unreadable)
        );
        assert_eq!(
            geometry_io_error(std::io::Error::from(std::io::ErrorKind::NotFound)).code,
            Some(Code::NotFound)
        );
        assert_eq!(
            geometry_io_error(std::io::Error::from(std::io::ErrorKind::PermissionDenied)).code,
            Some(Code::Unreadable)
        );
    }

    #[test]
    fn geometry_loader_distinguishes_missing_oversized_empty_and_unsupported_files() {
        use crate::read::GeometryErrorCode as Code;
        let dir = crate::unique_test_dir("viewer_error_codes");
        assert_eq!(
            load_geometry(&dir.join("missing.stl")).unwrap_err().code,
            Some(Code::NotFound)
        );
        assert_eq!(
            load_geometry(&dir.join("x.unknown")).unwrap_err().code,
            Some(Code::Unsupported)
        );
        let path = dir.join("empty.stl");
        std::fs::write(&path, b"solid empty\nendsolid empty").unwrap();
        assert_eq!(
            load_geometry(&path).unwrap_err().code,
            Some(Code::Unreadable)
        );
        let path = dir.join("too-large.obj");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(crate::safe_file::MAX_MODEL_FILE_BYTES + 1)
            .unwrap();
        assert_eq!(load_geometry(&path).unwrap_err().code, Some(Code::TooLarge));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn obj_preview_enforces_triangle_budget_without_limiting_import() {
        use crate::read::GeometryErrorCode as Code;
        let dir = crate::unique_test_dir("obj_preview_budget");
        let path = dir.join("repeated.obj");
        let vertices = "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\n";
        std::fs::write(&path, format!("{vertices}{}", "f 1 2 3\n".repeat(2))).unwrap();
        assert_eq!(
            load_geometry_with_obj_limit(&path, 3).unwrap()[0]
                .indices
                .len(),
            2
        );
        std::fs::write(&path, format!("{vertices}{}", "f 1 2 3\n".repeat(4))).unwrap();
        assert_eq!(
            load_geometry_with_obj_limit(&path, 3).unwrap_err().code,
            Some(Code::TooLarge)
        );
        assert!(obj::parse_obj_file(&path).is_ok());
        // Once over budget, later malformed lines must not be parsed.
        std::fs::write(
            &path,
            format!("{vertices}{}v broken\n", "f 1 2 3\n".repeat(4)),
        )
        .unwrap();
        assert_eq!(
            load_geometry_with_obj_limit(&path, 3).unwrap_err().code,
            Some(Code::TooLarge)
        );
        std::fs::write(&path, format!("{vertices}f 1 2 4 3\n")).unwrap();
        assert_eq!(
            load_geometry_with_obj_limit(&path, 2).unwrap()[0]
                .indices
                .len(),
            2
        );
        assert_eq!(
            load_geometry_with_obj_limit(&path, 1).unwrap_err().code,
            Some(Code::TooLarge)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn preview_check(path: &Path, ext: &str, tri: u64, ascii: u64, obj: u64) -> ReadResult<()> {
        let mut file = std::fs::File::open(path).unwrap();
        check_preview_size(&mut file, ext, tri, ascii, obj)
    }

    #[test]
    fn preview_size_check_uses_binary_stl_header_before_reading_the_rest() {
        use crate::read::GeometryErrorCode as Code;
        let dir = crate::unique_test_dir("preview_size_stl");
        // Only a header count that matches the file length counts as binary STL.
        let over = dir.join("over.stl");
        let mut head = vec![0u8; 80];
        head.extend_from_slice(&4u32.to_le_bytes());
        head.extend(std::iter::repeat_n(0u8, 4 * 50));
        std::fs::write(&over, &head).unwrap();
        assert_eq!(
            preview_check(&over, "stl", 3, 1 << 20, 1 << 20)
                .unwrap_err()
                .code,
            Some(Code::TooLarge)
        );
        assert!(preview_check(&over, "stl", 4, 1 << 20, 1 << 20).is_ok());

        // A count that does not match the length is treated as text and judged by bytes.
        let liar = dir.join("liar.stl");
        let mut head = vec![0u8; 80];
        head.extend_from_slice(&u32::MAX.to_le_bytes());
        std::fs::write(&liar, &head).unwrap();
        assert!(preview_check(&liar, "stl", 3, 1 << 20, 1 << 20).is_ok());
        assert_eq!(
            preview_check(&liar, "stl", 3, 10, 1 << 20)
                .unwrap_err()
                .code,
            Some(Code::TooLarge)
        );
    }

    #[test]
    fn preview_size_check_limits_ascii_stl_and_obj_by_bytes() {
        use crate::read::GeometryErrorCode as Code;
        let dir = crate::unique_test_dir("preview_size_text");
        let stl = dir.join("a.stl");
        std::fs::write(
            &stl,
            b"solid x
endsolid x
",
        )
        .unwrap();
        assert!(preview_check(&stl, "stl", 3, 100, 5).is_ok());
        assert_eq!(
            preview_check(&stl, "stl", 3, 10, 100).unwrap_err().code,
            Some(Code::TooLarge)
        );
        let obj = dir.join("a.obj");
        std::fs::write(&obj, b"v 0 0 0\n").unwrap();
        assert!(preview_check(&obj, "obj", 3, 5, 100).is_ok());
        assert_eq!(
            preview_check(&obj, "obj", 3, 100, 5).unwrap_err().code,
            Some(Code::TooLarge)
        );
    }

    #[test]
    fn small_binary_stl_still_loads_through_the_size_check() {
        let dir = crate::unique_test_dir("preview_size_small");
        let path = dir.join("one.stl");
        let mut bytes = vec![0u8; 80];
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 12]);
        for v in [[0f32, 0., 0.], [1., 0., 0.], [0., 1., 0.]] {
            for c in v {
                bytes.extend_from_slice(&c.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&[0u8; 2]);
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(load_geometry(&path).unwrap().len(), 1);
    }

    #[test]
    fn encode_render_meshes_handles_empty_mesh_list() {
        let bytes = encode_render_meshes(&[]);
        let decoded = decode_for_test(&bytes);
        assert!(decoded.is_empty());
    }
    #[test]
    fn creality_project_fixture_has_three_plates_and_five_meshes() {
        let bytes = include_bytes!("../../tests/fixtures/creality-3plates.3mf");
        let package =
            threemf::container::read_geometry_package(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(package.plate_assignments.objects.get("2"), Some(&1));
        for id in ["4", "6", "8"] {
            assert_eq!(package.plate_assignments.objects.get(id), Some(&2));
        }
        assert_eq!(package.plate_assignments.objects.get("10"), Some(&3));
        let meshes = threemf::extract_render_meshes(&package).unwrap();
        assert_eq!(
            meshes.iter().map(|m| m.plate).collect::<Vec<_>>(),
            vec![Some(1), Some(2), Some(2), Some(2), Some(3)]
        );
        let bytes = encode_render_meshes(&meshes);
        let header: serde_json::Value = serde_json::from_slice(
            &bytes[4..4 + u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize],
        )
        .unwrap();
        assert_eq!(header["plates"].as_array().unwrap().len(), 3);
    }
    #[test]
    fn synthetic_multiplate_package_propagates_root_ids_to_external_and_inline_meshes() {
        use std::io::{Cursor, Write};
        let mesh = "<mesh><vertices><vertex x=\"0\" y=\"0\" z=\"0\"/><vertex x=\"1\" y=\"0\" z=\"0\"/><vertex x=\"0\" y=\"1\" z=\"0\"/></vertices><triangles><triangle v1=\"0\" v2=\"1\" v3=\"2\"/></triangles></mesh>";
        let root = format!(
            r#"<model xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06"><resources>
            <object id="1" name="Same"><components><component objectid="10" p:path="/3D/Objects/part.model"/><component objectid="11" p:path="/3D/Objects/part.model"/></components></object>
            <object id="2" name="Same">{mesh}</object><object id="3" name="Same">{mesh}</object><object id="4" name="Same">{mesh}</object><object id="5">{mesh}</object>
            </resources><build><item objectid="1"/><item objectid="2"/><item objectid="3"/><item objectid="4"/><item objectid="5"/></build></model>"#
        );
        let external = format!("<model><resources><object id=\"10\">{mesh}</object><object id=\"11\">{mesh}</object></resources></model>");
        let config = r#"<config><plate><metadata key="plater_id" value="1"/><model_instance object_id="1"/><model_instance object_id="2"/></plate>
            <plate><metadata key="plater_id" value="2"/><metadata key="plater_name" value="Regalplatte"/><model_instance><metadata key="object_id" value="3"/></model_instance><model_instance object_id="4"/></plate>
            <plate><metadata key="plater_id" value="3"/></plate></config>"#;
        let generic_root = root.replace(
            r#"<components><component objectid="10" p:path="/3D/Objects/part.model"/><component objectid="11" p:path="/3D/Objects/part.model"/></components>"#, mesh)
            .replace(r#" xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06""#, "");
        let empty_root = format!("{}<build/></model>", root.split("<build>").next().unwrap());
        for (settings, model, expected) in [
            (
                config,
                root.as_str(),
                vec![Some(1), Some(1), Some(1), Some(2), Some(2), None],
            ),
            (
                config,
                generic_root.as_str(),
                vec![Some(1), Some(1), Some(2), Some(2), None],
            ),
            ("<config/>", root.as_str(), vec![None; 6]),
            (config, empty_root.as_str(), vec![]),
        ] {
            let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
            for (name, contents) in [
                ("3D/3dmodel.model", model),
                ("3D/Objects/part.model", external.as_str()),
                ("Metadata/model_settings.config", settings),
            ] {
                zip.start_file(name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(contents.as_bytes()).unwrap();
            }
            let package = threemf::container::read_geometry_package(zip.finish().unwrap()).unwrap();
            let meshes = threemf::extract_render_meshes(&package).unwrap();
            assert_eq!(meshes.iter().map(|m| m.plate).collect::<Vec<_>>(), expected);
            let bytes = encode_render_meshes(&meshes);
            let header: serde_json::Value = serde_json::from_slice(
                &bytes[4..4 + u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize],
            )
            .unwrap();
            if settings == config && !meshes.is_empty() {
                assert_eq!(
                    header["plates"],
                    serde_json::json!([{"number":1,"name":null},{"number":2,"name":"Regalplatte"}])
                );
                assert_eq!(header["meshes"][meshes.len() - 2]["plate"], 2);
            } else {
                assert!(header.get("plates").is_none());
            }
        }
    }
    #[test]
    fn geometry_header_contains_only_plates_with_geometry() {
        let mesh = |number, name: Option<&str>| RenderMesh {
            positions: vec![[0., 0., 0.]],
            indices: vec![[0, 0, 0]],
            plate: number,
            plate_name: name.map(str::to_string),
            ..Default::default()
        };
        let meshes = [
            mesh(Some(1), None),
            mesh(Some(2), Some("Regalplatte")),
            mesh(None, None),
        ];
        let bytes = encode_render_meshes(&meshes);
        let header: serde_json::Value = serde_json::from_slice(
            &bytes[4..4 + u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize],
        )
        .unwrap();
        assert_eq!(
            header["plates"],
            serde_json::json!([{"number":1,"name":null},{"number":2,"name":"Regalplatte"}])
        );
        assert_eq!(header["meshes"][0]["plate"], 1);
        assert_eq!(header["meshes"][2]["plate"], serde_json::Value::Null);
        let bytes = encode_render_meshes(&meshes[..1]);
        let header: serde_json::Value = serde_json::from_slice(
            &bytes[4..4 + u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize],
        )
        .unwrap();
        assert!(header.get("plates").is_none());
    }
}
