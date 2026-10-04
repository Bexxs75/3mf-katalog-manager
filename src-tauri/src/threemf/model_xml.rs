use std::collections::{BTreeMap, HashMap};

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use quick_xml::XmlVersion;

use super::error::ThreeMfError;
use super::geometry::Matrix3x4;

#[derive(Debug, Default, Clone)]
pub struct Mesh {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
    pub properties: Vec<(Option<String>, Option<usize>)>,
}

#[derive(Debug, Clone)]
pub struct Component {
    pub object_id: String,
    pub path: Option<String>,
    pub transform: Option<Matrix3x4>,
}

#[derive(Debug, Default, Clone)]
pub struct Object {
    pub object_type: Option<String>,
    pub name: Option<String>,
    pub pid: Option<String>,
    pub pindex: Option<usize>,
    pub mesh: Option<Mesh>,
    pub components: Vec<Component>,
}

#[derive(Debug, Clone)]
pub struct BuildItem {
    pub object_id: String,
    pub path: Option<String>,
    pub transform: Option<Matrix3x4>,
}

#[derive(Debug, Clone)]
pub struct Material {
    pub name: String,
    pub display_color: Option<String>,
}

#[derive(Debug, Default)]
pub struct ParsedModel {
    pub objects: HashMap<String, Object>,
    pub build_items: Vec<BuildItem>,
    pub metadata: BTreeMap<String, String>,
    pub materials: Vec<Material>,
    pub material_groups: HashMap<String, Vec<Material>>,
}

fn local_name(qname: &[u8]) -> &str {
    let s = std::str::from_utf8(qname).unwrap_or("");
    match s.rfind(':') {
        Some(idx) => &s[idx + 1..],
        None => s,
    }
}

fn get_attr(e: &BytesStart, name: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if local_name(a.key.as_ref()) == name {
            a.normalized_value(XmlVersion::Implicit1_0).ok().map(|v| v.into_owned())
        } else {
            None
        }
    })
}

#[derive(Default)]
struct ParseCtx {
    model: ParsedModel,
    current_object: Option<(String, Object)>,
    current_mesh: Option<Mesh>,
    in_vertices: bool,
    in_triangles: bool,
    current_basematerials: Option<Vec<Material>>,
    basematerials_id: String,
    pending_metadata_name: Option<String>,
    metadata_text: String,
}

fn handle_start(ctx: &mut ParseCtx, name: &str, e: &BytesStart) -> Result<(), ThreeMfError> {
    match name {
        "object" => {
            let id = get_attr(e, "id").unwrap_or_default();
            let object_type = get_attr(e, "type");
            ctx.current_object = Some((
                id,
                Object {
                    object_type,
                    name: get_attr(e, "name"),
                    pid: get_attr(e, "pid"),
                    pindex: get_attr(e, "pindex").and_then(|v| v.parse().ok()),
                    mesh: None,
                    components: Vec::new(),
                },
            ));
        }
        "mesh" => {
            ctx.current_mesh = Some(Mesh::default());
        }
        "vertices" => ctx.in_vertices = true,
        "vertex" => {
            if ctx.in_vertices {
                if let Some(mesh) = ctx.current_mesh.as_mut() {
                    let x = get_attr(e, "x").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    let y = get_attr(e, "y").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    let z = get_attr(e, "z").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    mesh.vertices.push([x, y, z]);
                }
            }
        }
        "triangles" => ctx.in_triangles = true,
        "triangle" => {
            if ctx.in_triangles {
                if let Some(mesh) = ctx.current_mesh.as_mut() {
                    // A missing or unreadable index must not silently become vertex 0.
                    let index = |name: &str| {
                        get_attr(e, name).and_then(|v| v.trim().parse::<u32>().ok()).ok_or_else(|| {
                            ThreeMfError::InvalidGeometry(format!("triangle without a valid {name}"))
                        })
                    };
                    mesh.triangles.push([index("v1")?, index("v2")?, index("v3")?]);
                    mesh.properties.push((get_attr(e, "pid"), get_attr(e, "p1").and_then(|v| v.parse().ok())));
                }
            }
        }
        "component" => {
            if let Some((_, obj)) = ctx.current_object.as_mut() {
                let object_id = get_attr(e, "objectid").unwrap_or_default();
                let path = get_attr(e, "path").map(|p| p.trim_start_matches('/').to_string());
                let transform = get_attr(e, "transform")
                    .map(|t| Matrix3x4::parse(&t))
                    .transpose()?;
                obj.components.push(Component {
                    object_id,
                    path,
                    transform,
                });
            }
        }
        "item" => {
            let object_id = get_attr(e, "objectid").unwrap_or_default();
            let path = get_attr(e, "path").map(|p| p.trim_start_matches('/').to_string());
            let transform = get_attr(e, "transform")
                .map(|t| Matrix3x4::parse(&t))
                .transpose()?;
            ctx.model.build_items.push(BuildItem {
                object_id,
                path,
                transform,
            });
        }
        "basematerials" => {
            ctx.basematerials_id = get_attr(e, "id").unwrap_or_default();
            ctx.current_basematerials = Some(Vec::new());
        }
        "base" => {
            if let Some(materials) = ctx.current_basematerials.as_mut() {
                let name = get_attr(e, "name").unwrap_or_default();
                let display_color = get_attr(e, "displaycolor");
                materials.push(Material {
                    name,
                    display_color,
                });
            }
        }
        "metadata" => {
            ctx.pending_metadata_name = get_attr(e, "name");
            ctx.metadata_text.clear();
        }
        _ => {}
    }
    Ok(())
}

fn handle_end(ctx: &mut ParseCtx, name: &str) {
    match name {
        "vertices" => ctx.in_vertices = false,
        "triangles" => ctx.in_triangles = false,
        "mesh" => {
            if let Some((_, obj)) = ctx.current_object.as_mut() {
                obj.mesh = ctx.current_mesh.take();
            }
        }
        "object" => {
            if let Some((id, obj)) = ctx.current_object.take() {
                ctx.model.objects.insert(id, obj);
            }
        }
        "basematerials" => {
            if let Some(materials) = ctx.current_basematerials.take() {
                ctx.model.materials.extend(materials.clone());
                ctx.model.material_groups.insert(ctx.basematerials_id.clone(), materials);
            }
        }
        "metadata" => {
            if let Some(key) = ctx.pending_metadata_name.take() {
                ctx.model
                    .metadata
                    .insert(key, ctx.metadata_text.trim().to_string());
            }
        }
        _ => {}
    }
}

pub fn parse_model_xml(xml: &str) -> Result<ParsedModel, ThreeMfError> {
    let mut reader = Reader::from_str(xml);
    let mut ctx = ParseCtx::default();

    loop {
        match reader.read_event()? {
            Event::Eof => break,
            Event::Start(e) => {
                let name = local_name(e.name().as_ref()).to_string();
                handle_start(&mut ctx, &name, &e)?;
            }
            Event::Empty(e) => {
                let name = local_name(e.name().as_ref()).to_string();
                handle_start(&mut ctx, &name, &e)?;
                handle_end(&mut ctx, &name);
            }
            Event::Text(t) => {
                if ctx.pending_metadata_name.is_some() {
                    ctx.metadata_text.push_str(&t.decode().map_err(quick_xml::Error::from)?);
                }
            }
            Event::End(e) => {
                let name = local_name(e.name().as_ref()).to_string();
                handle_end(&mut ctx, &name);
            }
            _ => {}
        }
    }

    for (id, object) in &ctx.model.objects {
        if let Some(mesh) = &object.mesh {
            validate_mesh(id, mesh)?;
        }
    }
    Ok(ctx.model)
}

/// Every consumer (volume, bounding box, render meshes) indexes `vertices` with
/// the triangle indices, so a model from an untrusted file is checked once here.
fn validate_mesh(object_id: &str, mesh: &Mesh) -> Result<(), ThreeMfError> {
    if mesh.vertices.iter().flatten().any(|c| !c.is_finite()) {
        return Err(ThreeMfError::InvalidGeometry(format!("object {object_id}: non-finite coordinate")));
    }
    let vertex_count = mesh.vertices.len();
    if let Some(bad) = mesh.triangles.iter().flatten().find(|&&i| i as usize >= vertex_count) {
        return Err(ThreeMfError::InvalidGeometry(format!(
            "object {object_id}: vertex index {bad} with {vertex_count} vertices"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_p_path_on_component_and_item_and_normalizes_leading_slash() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06">
  <resources>
    <object id="1" type="model">
      <components>
        <component p:path="/3D/Objects/object_2.model" objectid="2"/>
      </components>
    </object>
  </resources>
  <build>
    <item p:path="/3D/Objects/object_1.model" objectid="5"/>
    <item objectid="1"/>
  </build>
</model>"##;

        let model = parse_model_xml(xml).expect("parse should succeed");

        let component = &model.objects.get("1").expect("object 1 present").components[0];
        assert_eq!(component.object_id, "2");
        assert_eq!(
            component.path.as_deref(),
            Some("3D/Objects/object_2.model")
        );

        assert_eq!(
            model.build_items[0].path.as_deref(),
            Some("3D/Objects/object_1.model")
        );
        assert_eq!(model.build_items[1].path, None);
    }

    fn mesh_model(vertices: &str, triangles: &str) -> String {
        format!(
            r#"<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources><object id="1" type="model"><mesh><vertices>{vertices}</vertices><triangles>{triangles}</triangles></mesh></object></resources><build><item objectid="1"/></build></model>"#
        )
    }

    const THREE_VERTICES: &str = r#"<vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/>"#;

    #[test]
    fn valid_triangle_is_accepted() {
        let model = parse_model_xml(&mesh_model(THREE_VERTICES, r#"<triangle v1="0" v2="1" v3="2"/>"#))
            .expect("valid mesh");
        assert_eq!(model.objects["1"].mesh.as_ref().unwrap().triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn triangle_index_past_the_last_vertex_is_rejected() {
        let xml = mesh_model(r#"<vertex x="0" y="0" z="0"/>"#, r#"<triangle v1="0" v2="1" v3="0"/>"#);
        assert!(matches!(parse_model_xml(&xml), Err(ThreeMfError::InvalidGeometry(_))));
    }

    #[test]
    fn triangle_with_huge_index_is_rejected() {
        let xml = mesh_model(THREE_VERTICES, r#"<triangle v1="0" v2="1" v3="4294967295"/>"#);
        assert!(matches!(parse_model_xml(&xml), Err(ThreeMfError::InvalidGeometry(_))));
    }

    #[test]
    fn triangle_without_vertices_is_rejected() {
        let xml = mesh_model("", r#"<triangle v1="0" v2="0" v3="0"/>"#);
        assert!(matches!(parse_model_xml(&xml), Err(ThreeMfError::InvalidGeometry(_))));
    }

    #[test]
    fn missing_or_negative_index_is_rejected_instead_of_becoming_zero() {
        for triangle in [r#"<triangle v1="0" v3="2"/>"#, r#"<triangle v1="0" v2="-1" v3="2"/>"#] {
            let xml = mesh_model(THREE_VERTICES, triangle);
            assert!(
                matches!(parse_model_xml(&xml), Err(ThreeMfError::InvalidGeometry(_))),
                "{triangle}"
            );
        }
    }

    #[test]
    fn non_finite_coordinate_is_rejected() {
        let vertices = r#"<vertex x="0" y="0" z="0"/><vertex x="NaN" y="0" z="0"/><vertex x="0" y="inf" z="0"/>"#;
        let xml = mesh_model(vertices, r#"<triangle v1="0" v2="1" v3="2"/>"#);
        assert!(matches!(parse_model_xml(&xml), Err(ThreeMfError::InvalidGeometry(_))));
    }
}

#[cfg(test)]
mod material_tests {
    use super::*;

    #[test]
    fn records_object_and_triangle_properties_and_group_ids() {
        let model = parse_model_xml(r##"<model><resources>
          <basematerials id="7"><base name="Red" displaycolor="#FF000080"/></basematerials>
          <object id="1" name="Body" pid="7" pindex="0"><mesh><vertices>
          <vertex x="0"/><vertex x="1"/><vertex y="1"/>
          </vertices><triangles><triangle v1="0" v2="1" v3="2" pid="8" p1="1"/>
          </triangles></mesh></object></resources></model>"##).unwrap();
        let obj = &model.objects["1"];
        assert_eq!(obj.name.as_deref(), Some("Body"));
        assert_eq!(obj.pid.as_deref(), Some("7"));
        assert_eq!(obj.pindex, Some(0));
        assert_eq!(obj.mesh.as_ref().unwrap().properties, vec![(Some("8".into()), Some(1))]);
        assert_eq!(model.material_groups["7"][0].display_color.as_deref(), Some("#FF000080"));
    }
}
