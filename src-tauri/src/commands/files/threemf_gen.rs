//! Synthetic 3MF archives for the large-catalog and import-speed measurements. Every value is
//! invented (a strip of triangles, neutral numbers); the layouts only imitate the structure of
//! the slicers' project files. Nothing here is derived from a real file.
use std::io::Write;
use std::path::Path;

/// The structures the measurements mix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// Bambu structure with `<plate>` blocks (1 to 4 plates), project settings, `plate_N.json`.
    Sliced,
    /// Bambu structure whose `slice_info.config` has a header only, plus project settings.
    NoPrintData,
    /// PrusaSlicer project structure (`Slic3r_PE.config`, no `slice_info.config`).
    Prusa,
    /// A plain 3MF without any slicer entry.
    Plain,
    /// A valid archive cut in half (no central directory).
    Broken,
    /// A file of 0 bytes.
    Empty,
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#;
const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Target="/3D/3dmodel.model" Id="rel-1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>"#;

/// A strip of `triangles` triangles; `seed` shifts it so every file differs (the hash is unique).
fn model_xml(triangles: usize, seed: usize) -> String {
    let mut vertices = String::new();
    let mut faces = String::new();
    for i in 0..triangles {
        let x = (i + seed * 7) as f64 * 0.01;
        vertices.push_str(&format!(r#"<vertex x="{x}" y="0" z="0"/><vertex x="{x}" y="1" z="0"/><vertex x="{x}" y="0" z="1"/>"#));
        faces.push_str(&format!(r#"<triangle v1="{}" v2="{}" v3="{}"/>"#, i * 3, i * 3 + 1, i * 3 + 2));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources><object id="1" type="model"><mesh><vertices>{vertices}</vertices><triangles>{faces}</triangles></mesh></object></resources><build><item objectid="1"/></build></model>"#
    )
}

/// Project settings as JSON with the shapes of a Bambu file (9 filaments, 17 nozzle values) and
/// invented padding keys up to about `pad_bytes`, so the parse cost resembles a real project.
fn project_settings(pad_bytes: usize) -> String {
    let nine = |v: &str| format!("[{}]", (0..9).map(|_| format!("\"{v}\"")).collect::<Vec<_>>().join(","));
    let seventeen = |v: &str| format!("[{}]", (0..17).map(|_| format!("\"{v}\"")).collect::<Vec<_>>().join(","));
    // Nine invented filament colors (the last one with alpha, as the slicers write them).
    let colors = format!("[{}]", (0..9).map(|i| format!("\"#{:02X}{:02X}{:02X}{}\"", 20 + i * 20, 200 - i * 10, 40 + i * 15, if i == 8 { "FF" } else { "" })).collect::<Vec<_>>().join(","));
    let mut json = format!(
        r#"{{"from":"project","printer_model":"Invented Printer","curr_bed_type":"Supertack Plate","layer_height":"0.2","nozzle_diameter":["0.4"],"filament_type":{},"filament_colour":{},"nozzle_temperature":{},"nozzle_temperature_initial_layer":{},"supertack_plate_temp":{},"hot_plate_temp":{},"cool_plate_temp":{}"#,
        nine("PLA"),
        colors,
        seventeen("210"),
        seventeen("215"),
        nine("45"),
        nine("55"),
        nine("35")
    );
    let mut k = 0;
    while json.len() < pad_bytes {
        json.push_str(&format!(r#","invented_option_{k}":"value_{k}_abcdefghij""#));
        k += 1;
    }
    json.push('}');
    json
}

fn slice_info(plates: usize, with_plates: bool) -> String {
    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?><config><header><header_item key="X-BBL-Client-Type" value="slicer"/></header>"#);
    if with_plates {
        for n in 1..=plates {
            xml.push_str(&format!(
                r##"<plate><metadata key="index" value="{n}"/><metadata key="weight" value="{}.5"/><object identify_id="1" name="Shape" skipped="false"/><filament id="1" type="PLA" color="#FFFFFFFF" used_m="{}.1" used_g="{}.2"/></plate>"##,
                10 + n,
                3 + n,
                9 + n
            ));
        }
    }
    xml.push_str("</config>");
    xml
}

fn model_settings(plates: usize) -> String {
    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?><config><object id="1"><metadata key="name" value="shape"/><metadata key="extruder" value="2"/><part id="1" subtype="normal_part"><metadata key="extruder" value="3"/></part></object>"#);
    for n in 1..=plates {
        xml.push_str(&format!(r#"<plate><metadata key="plater_id" value="{n}"/></plate>"#));
    }
    xml.push_str("</config>");
    xml
}

/// The entries of a valid archive of `kind`, as (name, bytes).
fn entries(kind: Kind, seed: usize, triangles: usize) -> Vec<(String, Vec<u8>)> {
    let mut list: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".into(), CONTENT_TYPES.as_bytes().to_vec()),
        ("_rels/.rels".into(), RELS.as_bytes().to_vec()),
        ("3D/3dmodel.model".into(), model_xml(triangles, seed).into_bytes()),
    ];
    let plates = 1 + seed % 4;
    match kind {
        Kind::Sliced => {
            list.push(("Metadata/model_settings.config".into(), model_settings(plates).into_bytes()));
            list.push(("Metadata/slice_info.config".into(), slice_info(plates, true).into_bytes()));
            list.push(("Metadata/project_settings.config".into(), project_settings(88_000).into_bytes()));
            for n in 1..=plates {
                let body = format!(r#"{{"bed_type":"supertack_plate","first_layer_time":{}.25,"bbox_objects":[],"version":2}}"#, 100 + n);
                list.push((format!("Metadata/plate_{n}.json"), body.into_bytes()));
            }
        }
        Kind::NoPrintData => {
            list.push(("Metadata/model_settings.config".into(), model_settings(1).into_bytes()));
            list.push(("Metadata/slice_info.config".into(), slice_info(0, false).into_bytes()));
            list.push(("Metadata/project_settings.config".into(), project_settings(88_000).into_bytes()));
        }
        Kind::Prusa => {
            let mut config = String::from("; invented PrusaSlicer-style settings\n");
            for k in 0..400 {
                config.push_str(&format!("; invented_option_{k} = value_{k}\n"));
            }
            config.push_str("; layer_height = 0.2\n; nozzle_diameter = 0.4\n; temperature = 215\n; bed_temperature = 60\n; printer_model = INVENTED\n");
            list.push(("Metadata/Slic3r_PE.config".into(), config.into_bytes()));
        }
        Kind::Plain | Kind::Broken | Kind::Empty => {}
    }
    list
}

fn zip_bytes(list: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in list {
            zip.start_file(name.as_str(), options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    buf
}

/// Writes one synthetic file. `seed` makes it unique; `triangles` sizes the model.
pub(super) fn write_file(path: &Path, kind: Kind, seed: usize, triangles: usize) {
    let bytes = match kind {
        Kind::Empty => Vec::new(),
        Kind::Broken => {
            let valid = zip_bytes(&entries(Kind::Sliced, seed, triangles));
            valid[..valid.len() / 2].to_vec()
        }
        other => zip_bytes(&entries(other, seed, triangles)),
    };
    std::fs::write(path, bytes).unwrap();
}

/// A sliced-looking archive whose `project_settings.config` is `settings_bytes` of padding.
pub(super) fn write_with_huge_settings(path: &Path, settings_bytes: usize) {
    let mut list = entries(Kind::Sliced, 1, 10);
    for (name, bytes) in list.iter_mut() {
        if name == "Metadata/project_settings.config" {
            *bytes = format!(r#"{{"layer_height":"0.2","pad":"{}"}}"#, "x".repeat(settings_bytes)).into_bytes();
        }
    }
    std::fs::write(path, zip_bytes(&list)).unwrap();
}

/// The mix of the large catalog by position: 40 % sliced, 25 % without print data, 10 % Prusa,
/// 20 % plain, 4 % broken, 1 % empty.
pub(super) fn kind_for_position(i: usize) -> Kind {
    match i % 100 {
        0..=39 => Kind::Sliced,
        40..=64 => Kind::NoPrintData,
        65..=74 => Kind::Prusa,
        75..=94 => Kind::Plain,
        95..=98 => Kind::Broken,
        _ => Kind::Empty,
    }
}
