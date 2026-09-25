use super::error::ObjError;

type ObjGeometry = (Vec<[f64; 3]>, Vec<[u32; 3]>);

/// Handgeschriebener Wavefront-OBJ-Parser (wie der STL-Parser, ohne Crate).
/// `vt`/`vn`, `mtllib`/`usemtl` und `o`/`g`/`s` werden ignoriert; Normalen
/// entstehen spaeter aus der Geometrie.
///
/// Achsen: OBJ ist praktisch immer Y-up, intern gilt Z-up (`ModelViewer.tsx`
/// dreht JEDE Geometrie per `rotateX(-PI/2)`). Deshalb wird hier jeder Vertex
/// `(x, y, z)` zu `(x, -z, y)`, die Umkehrung dieser Drehung (keine Spiegelung).
pub fn parse(text: &str) -> Result<ObjGeometry, ObjError> {
    let mut vertices: Vec<[f64; 3]> = Vec::new();
    let mut triangles: Vec<[u32; 3]> = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut tokens = line.split_whitespace();
        let Some(keyword) = tokens.next() else { continue };

        match keyword {
            "v" => {
                let coords: Vec<f64> = tokens
                    .take(3)
                    .map(|t| t.parse::<f64>())
                    .collect::<Result<_, _>>()
                    .map_err(|_| ObjError::Parse(format!("invalid vertex coordinates: {line}")))?;
                if coords.len() != 3 {
                    return Err(ObjError::Parse(format!("invalid vertex coordinates: {line}")));
                }
                // Y-up (Datei) -> internes Z-up (siehe Doc-Kommentar oben).
                vertices.push([coords[0], -coords[2], coords[1]]);
            }
            "f" => {
                let face_indices: Vec<u32> = line
                    .split_whitespace()
                    .skip(1)
                    .map(|token| resolve_index(token, vertices.len(), line))
                    .collect::<Result<_, _>>()?;
                if face_indices.len() < 3 {
                    return Err(ObjError::Parse(format!("face needs at least 3 vertices: {line}")));
                }
                // Fan-Triangulierung fuer N-Gon-Faces (Quads etc.) - Standard-
                // Annahme fuer einfache OBJ-Loader, konvex vorausgesetzt.
                for i in 1..face_indices.len() - 1 {
                    triangles.push([face_indices[0], face_indices[i], face_indices[i + 1]]);
                }
            }
            // vt/vn/o/g/s/mtllib/usemtl und alles Unbekannte: bewusst ignoriert.
            _ => {}
        }
    }

    if vertices.is_empty() {
        return Err(ObjError::Parse("no vertices found".to_string()));
    }

    Ok((vertices, triangles))
}

/// Ein Face-Token hat die Form `v`, `v/vt`, `v//vn` oder `v/vt/vn` - nur der
/// erste (Vertex-)Teil wird gebraucht. Der Index kann laut OBJ-Spezifikation
/// auch negativ/relativ sein (`-1` = zuletzt gelesener Vertex VOR dieser
/// Face-Zeile) - in echten Exporten verbreitet genug, dass ein Parser ohne
/// diese Unterstuetzung an realen Dateien scheitern wuerde.
fn resolve_index(token: &str, vertex_count: usize, line: &str) -> Result<u32, ObjError> {
    let vertex_part = token.split('/').next().unwrap_or(token);
    let raw: i64 = vertex_part
        .parse()
        .map_err(|_| ObjError::Parse(format!("invalid face index '{token}' in: {line}")))?;

    let resolved = if raw < 0 {
        vertex_count as i64 + raw
    } else {
        raw - 1
    };

    if resolved < 0 || resolved >= vertex_count as i64 {
        return Err(ObjError::Parse(format!("face index out of range '{token}' in: {line}")));
    }
    Ok(resolved as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRIANGLE_OBJ: &str = "\
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 0.0 1.0 0.0
f 1 2 3
";

    #[test]
    fn parses_a_single_triangle() {
        let (vertices, triangles) = parse(TRIANGLE_OBJ).unwrap();
        // Datei-Vertex (0,1,0) (oben in Y-up) wird intern (0,0,1).
        assert_eq!(vertices, vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]);
        assert_eq!(triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn parses_faces_with_texture_and_normal_indices() {
        let obj = "\
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 0.0 1.0 0.0
vt 0.0 0.0
vn 0.0 0.0 1.0
f 1/1/1 2/1/1 3/1/1
";
        let (_, triangles) = parse(obj).unwrap();
        assert_eq!(triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn parses_faces_with_vertex_and_normal_only() {
        let obj = "\
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 0.0 1.0 0.0
vn 0.0 0.0 1.0
f 1//1 2//1 3//1
";
        let (_, triangles) = parse(obj).unwrap();
        assert_eq!(triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn normalizes_obj_y_up_to_the_apps_internal_z_up_convention() {
        // "1 nach oben" in der Datei muss intern "1 in Z" sein, sonst kippt das Modell.
        let obj = "v 0.0 1.0 0.0\nv 0.0 0.0 0.0\nv 1.0 0.0 0.0\nf 1 2 3\n";
        let (vertices, _) = parse(obj).unwrap();
        assert_eq!(vertices[0], [0.0, 0.0, 1.0]);
    }
    #[test]
    fn fan_triangulates_a_quad_face() {
        let obj = "\
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 1.0 1.0 0.0
v 0.0 1.0 0.0
f 1 2 3 4
";
        let (_, triangles) = parse(obj).unwrap();
        assert_eq!(triangles, vec![[0, 1, 2], [0, 2, 3]]);
    }

    #[test]
    fn resolves_negative_relative_face_indices() {
        // -1/-2/-3 beziehen sich auf die zuletzt gelesenen 3 Vertices, hier
        // aequivalent zu "1 2 3" bei genau 3 vorhandenen Vertices.
        let obj = "\
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 0.0 1.0 0.0
f -3 -2 -1
";
        let (_, triangles) = parse(obj).unwrap();
        assert_eq!(triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn ignores_comments_groups_and_material_directives() {
        let obj = "\
# a comment
mtllib scene.mtl
o Cube
g default
usemtl Material
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 0.0 1.0 0.0
s off
f 1 2 3
";
        let (vertices, triangles) = parse(obj).unwrap();
        assert_eq!(vertices.len(), 3);
        assert_eq!(triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn rejects_a_face_with_fewer_than_three_vertices() {
        let obj = "\
v 0.0 0.0 0.0
v 1.0 0.0 0.0
f 1 2
";
        assert!(parse(obj).is_err());
    }

    #[test]
    fn rejects_an_out_of_range_face_index() {
        let obj = "\
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 0.0 1.0 0.0
f 1 2 9
";
        assert!(parse(obj).is_err());
    }

    #[test]
    fn rejects_invalid_vertex_coordinates() {
        let obj = "v x y z\nf 1 1 1\n";
        assert!(parse(obj).is_err());
    }

    #[test]
    fn rejects_a_file_with_no_vertices() {
        assert!(parse("# just a comment\n").is_err());
    }
}
