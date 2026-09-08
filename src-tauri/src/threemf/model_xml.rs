use std::collections::{BTreeMap, HashMap};

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use super::error::ThreeMfError;
use super::geometry::Matrix3x4;

#[derive(Debug, Default, Clone)]
pub struct Mesh {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

#[derive(Debug, Clone)]
pub struct Component {
    pub object_id: String,
    pub transform: Option<Matrix3x4>,
}

#[derive(Debug, Default, Clone)]
pub struct Object {
    pub object_type: Option<String>,
    pub mesh: Option<Mesh>,
    pub components: Vec<Component>,
}

#[derive(Debug, Clone)]
pub struct BuildItem {
    pub object_id: String,
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
            a.unescape_value().ok().map(|v| v.into_owned())
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
                    let v1 = get_attr(e, "v1").and_then(|v| v.parse().ok()).unwrap_or(0);
                    let v2 = get_attr(e, "v2").and_then(|v| v.parse().ok()).unwrap_or(0);
                    let v3 = get_attr(e, "v3").and_then(|v| v.parse().ok()).unwrap_or(0);
                    mesh.triangles.push([v1, v2, v3]);
                }
            }
        }
        "component" => {
            if let Some((_, obj)) = ctx.current_object.as_mut() {
                let object_id = get_attr(e, "objectid").unwrap_or_default();
                let transform = get_attr(e, "transform")
                    .map(|t| Matrix3x4::parse(&t))
                    .transpose()?;
                obj.components.push(Component {
                    object_id,
                    transform,
                });
            }
        }
        "item" => {
            let object_id = get_attr(e, "objectid").unwrap_or_default();
            let transform = get_attr(e, "transform")
                .map(|t| Matrix3x4::parse(&t))
                .transpose()?;
            ctx.model.build_items.push(BuildItem {
                object_id,
                transform,
            });
        }
        "basematerials" => {
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
                ctx.model.materials.extend(materials);
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
                    ctx.metadata_text.push_str(&t.unescape()?);
                }
            }
            Event::End(e) => {
                let name = local_name(e.name().as_ref()).to_string();
                handle_end(&mut ctx, &name);
            }
            _ => {}
        }
    }

    Ok(ctx.model)
}
