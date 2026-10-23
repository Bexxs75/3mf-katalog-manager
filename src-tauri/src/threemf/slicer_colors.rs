//! Filament colors of Bambu Studio / OrcaSlicer projects. These slicers do not
//! write 3MF `<basematerials>`: each object and part names an extruder in
//! `Metadata/model_settings.config`, and the filament colors of the project
//! are a list in `Metadata/project_settings.config` (extruder n uses entry
//! n - 1). Both files are optional slicer metadata, so anything missing or
//! malformed only means "no colors", never a failed preview.

use std::collections::HashMap;
use std::io::{Read, Seek};

use quick_xml::events::Event;
use zip::ZipArchive;

use crate::geometry::RenderColor;

const PROJECT_SETTINGS_PATH: &str = "Metadata/project_settings.config";
const MODEL_SETTINGS_PATH: &str = "Metadata/model_settings.config";
/// Far above any real printer setup; bounds the per-instance work of the
/// preview against a crafted color list. Later entries count as unusable.
pub const MAX_FILAMENTS: usize = 256;

#[derive(Debug, Clone, Default)]
pub struct SlicerColors {
    /// Index = filament number - 1; `None` for an entry that is no usable color.
    pub filaments: Vec<Option<RenderColor>>,
    /// Keyed by the object id of a root-model build item.
    pub objects: HashMap<String, ObjectExtruders>,
}

#[derive(Debug, Clone, Default)]
pub struct ObjectExtruders {
    pub extruder: Option<u32>,
    /// Keyed by part id, which the slicers write as the object id of the
    /// component that holds the part's mesh.
    pub parts: HashMap<String, u32>,
}

impl SlicerColors {
    /// Extruder of a mesh: the part's own setting, else its object's, else
    /// the first extruder, like the slicers resolve it.
    pub fn extruder(&self, object_id: Option<&str>, part_id: Option<&str>) -> u32 {
        let Some(object) = object_id.and_then(|id| self.objects.get(id)) else { return 1 };
        part_id.and_then(|id| object.parts.get(id).copied()).or(object.extruder).unwrap_or(1)
    }

    pub fn has_colors(&self) -> bool {
        self.filaments.iter().any(Option::is_some)
    }
}

/// Reads both configuration files; returns the bytes read for the package budget.
pub fn read_slicer_colors<R: Read + Seek>(archive: &mut ZipArchive<R>) -> (SlicerColors, u64) {
    let mut bytes = 0;
    let mut colors = SlicerColors::default();
    if let Ok(json) = super::container::read_entry_to_string(archive, PROJECT_SETTINGS_PATH, super::container::MAX_CONFIG_XML_BYTES) {
        bytes += json.len() as u64;
        colors.filaments = parse_filament_colors(&json);
    }
    if let Ok(xml) = super::container::read_entry_to_string(archive, MODEL_SETTINGS_PATH, super::container::MAX_CONFIG_XML_BYTES) {
        bytes += xml.len() as u64;
        colors.objects = parse_object_extruders(&xml);
    }
    (colors, bytes)
}

fn parse_filament_colors(json: &str) -> Vec<Option<RenderColor>> {
    let Ok(settings) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    let list = |key: &str| settings.get(key).and_then(serde_json::Value::as_array).cloned().unwrap_or_default();
    let types = list("filament_type");
    list("filament_colour").iter().take(MAX_FILAMENTS).enumerate().map(|(index, color)| {
        let color = super::normalize_color(color.as_str()?.trim())?;
        let name = super::display_name(types.get(index).and_then(serde_json::Value::as_str).unwrap_or("").trim());
        Some(RenderColor { name, color, filament: Some(index as u32 + 1) })
    }).collect()
}

/// Extruder 0 means "inherit" in the slicers, so it is stored as unset.
fn parse_object_extruders(xml: &str) -> HashMap<String, ObjectExtruders> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut objects = HashMap::new();
    let mut object: Option<(String, ObjectExtruders)> = None;
    let mut part: Option<String> = None;
    let attribute = |element: &quick_xml::events::BytesStart, name: &[u8], reader: &quick_xml::Reader<&[u8]>| {
        element.attributes().flatten().find(|a| a.key.local_name().as_ref() == name).and_then(|a| {
            a.decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, reader.decoder()).ok().map(|v| v.into_owned())
        })
    };
    loop {
        match reader.read_event() {
            Ok(event @ (Event::Start(_) | Event::Empty(_))) => {
                let (element, opens) = match event {
                    Event::Start(e) => (e, true),
                    Event::Empty(e) => (e, false),
                    _ => unreachable!(),
                };
                match element.local_name().as_ref() {
                    b"object" if opens => {
                        object = attribute(&element, b"id", &reader).map(|id| (id, ObjectExtruders::default()));
                    }
                    b"part" if opens => part = attribute(&element, b"id", &reader),
                    b"metadata" => {
                        let Some((_, extruders)) = &mut object else { continue };
                        if attribute(&element, b"key", &reader).as_deref() != Some("extruder") { continue; }
                        let Some(value) = attribute(&element, b"value", &reader)
                            .and_then(|v| v.trim().parse::<u32>().ok()).filter(|v| *v > 0) else { continue };
                        match &part {
                            Some(id) => { extruders.parts.insert(id.clone(), value); }
                            None => extruders.extruder = Some(value),
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"part" => part = None,
                b"object" => {
                    if let Some((id, extruders)) = object.take() { objects.insert(id, extruders); }
                    part = None;
                }
                _ => {}
            },
            Ok(Event::Eof) => return objects,
            Err(_) => return HashMap::new(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filament_colors_keep_their_slot_and_skip_unusable_entries() {
        let colors = parse_filament_colors(r##"{"filament_colour": ["#F88343", "oops", "#00000080", 7],
            "filament_type": ["PLA", "PETG"]}"##);
        assert_eq!(colors, vec![
            Some(RenderColor { name: "PLA".into(), color: "#f88343".into(), filament: Some(1) }),
            None,
            Some(RenderColor { name: String::new(), color: "#000000".into(), filament: Some(3) }),
            None,
        ]);
        for json in ["", "[]", "{}", r##"{"filament_colour": "#FFFFFF"}"##, "{broken"] {
            assert!(parse_filament_colors(json).is_empty(), "{json}");
        }
    }

    #[test]
    fn overlong_color_lists_are_cut_at_the_filament_limit() {
        let json = format!(r##"{{"filament_colour": [{}"#FFFFFF"]}}"##, "null,".repeat(1_000_000));
        let colors = parse_filament_colors(&json);
        assert_eq!(colors.len(), MAX_FILAMENTS);
        assert!(colors.iter().all(Option::is_none));
    }

    #[test]
    fn extruders_resolve_part_then_object_then_first() {
        let colors = SlicerColors { objects: parse_object_extruders(r#"<config>
          <object id="7"><metadata key="name" value="Duck"/><metadata key="extruder" value="2"/>
            <part id="1" subtype="normal_part"><metadata key="extruder" value="3"/></part>
            <part id="2"><metadata key="extruder" value="0"/></part></object>
          <object id="9"><part id="1"><metadata key="extruder" value="4"/></part></object>
          <plate><metadata key="extruder" value="5"/><model_instance><metadata key="object_id" value="7"/></model_instance></plate>
        </config>"#), ..Default::default() };
        assert_eq!(colors.extruder(Some("7"), Some("1")), 3);
        assert_eq!(colors.extruder(Some("7"), Some("2")), 2);
        assert_eq!(colors.extruder(Some("7"), None), 2);
        assert_eq!(colors.extruder(Some("9"), Some("1")), 4);
        assert_eq!(colors.extruder(Some("9"), Some("2")), 1);
        assert_eq!(colors.extruder(Some("404"), Some("1")), 1);
        assert_eq!(colors.extruder(None, None), 1);
    }

    #[test]
    fn malformed_model_settings_have_no_extruders() {
        for xml in ["<config><object id='1'><metadata key='extruder' value='2'/></object></wrong>", "<<<"] {
            assert!(parse_object_extruders(xml).is_empty(), "{xml}");
        }
    }
}
