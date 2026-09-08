use std::io::{Read, Seek};

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use zip::ZipArchive;

use super::error::ThreeMfError;

const RELS_PATH: &str = "_rels/.rels";
const MODEL_RELATIONSHIP_TYPE: &str =
    "http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel";
const THUMBNAIL_RELATIONSHIP_TYPE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail";
const DEFAULT_MODEL_PATH: &str = "3D/3dmodel.model";
const FALLBACK_THUMBNAIL_PATHS: [&str; 2] =
    ["Metadata/thumbnail.png", "3D/Thumbnails/thumbnail.png"];

pub struct PackageParts {
    pub model_xml: String,
    pub thumbnail: Option<Vec<u8>>,
}

pub fn read_package<R: Read + Seek>(reader: R) -> Result<PackageParts, ThreeMfError> {
    let mut archive = ZipArchive::new(reader)?;

    let (model_path, thumbnail_path) = resolve_relationships(&mut archive);
    let model_path = model_path.unwrap_or_else(|| DEFAULT_MODEL_PATH.to_string());

    let model_xml = read_entry_to_string(&mut archive, &model_path)
        .or_else(|_| read_entry_to_string(&mut archive, DEFAULT_MODEL_PATH))
        .map_err(|_| ThreeMfError::MissingRootModel)?;

    let mut thumbnail = thumbnail_path.and_then(|p| read_entry_to_bytes(&mut archive, &p).ok());
    if thumbnail.is_none() {
        for candidate in FALLBACK_THUMBNAIL_PATHS {
            if let Ok(bytes) = read_entry_to_bytes(&mut archive, candidate) {
                thumbnail = Some(bytes);
                break;
            }
        }
    }

    Ok(PackageParts {
        model_xml,
        thumbnail,
    })
}

fn resolve_relationships<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> (Option<String>, Option<String>) {
    let Ok(rels_xml) = read_entry_to_string(archive, RELS_PATH) else {
        return (None, None);
    };

    let mut model_path = None;
    let mut thumbnail_path = None;

    let mut reader = Reader::from_str(&rels_xml);
    loop {
        let event = match reader.read_event() {
            Ok(event) => event,
            Err(_) => break,
        };
        match event {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e) => {
                if e.name().as_ref() == b"Relationship" {
                    let mut rel_type = None;
                    let mut target = None;
                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            b"Type" => rel_type = attr.unescape_value().ok().map(|v| v.into_owned()),
                            b"Target" => target = attr.unescape_value().ok().map(|v| v.into_owned()),
                            _ => {}
                        }
                    }
                    if let (Some(rel_type), Some(target)) = (rel_type, target) {
                        let normalized = target.trim_start_matches('/').to_string();
                        if rel_type == MODEL_RELATIONSHIP_TYPE {
                            model_path = Some(normalized);
                        } else if rel_type == THUMBNAIL_RELATIONSHIP_TYPE {
                            thumbnail_path = Some(normalized);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    (model_path, thumbnail_path)
}

fn read_entry_to_string<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
) -> Result<String, ThreeMfError> {
    let mut file = archive.by_name(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

fn read_entry_to_bytes<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
) -> Result<Vec<u8>, ThreeMfError> {
    let mut file = archive.by_name(path)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;
    Ok(contents)
}
