use super::error::StlError;

const BINARY_HEADER_LEN: usize = 80;
const BINARY_FACET_LEN: usize = 50;

type StlGeometry = (Vec<[f64; 3]>, Vec<[u32; 3]>);

pub fn parse(bytes: &[u8]) -> Result<StlGeometry, StlError> {
    if is_binary(bytes) {
        parse_binary(bytes)
    } else {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| StlError::Parse("not valid ASCII/UTF-8 STL text".to_string()))?;
        parse_ascii(text)
    }
}

// A binary STL's size is fully determined by its facet count, so matching
// that exactly is a more reliable format check than the conventional (but
// non-mandatory) "solid" text prefix, which some binary exporters also emit.
fn is_binary(bytes: &[u8]) -> bool {
    let Some(count) = read_facet_count(bytes) else {
        return false;
    };
    bytes.len() == BINARY_HEADER_LEN + 4 + count * BINARY_FACET_LEN
}

// Bounds-geprueft statt bytes[80..84] + unwrap(): so gibt es auch ohne
// vorheriges is_binary() einen Fehler statt einer Panik.
fn read_facet_count(bytes: &[u8]) -> Option<usize> {
    let slice = bytes.get(BINARY_HEADER_LEN..BINARY_HEADER_LEN + 4)?;
    Some(u32::from_le_bytes(slice.try_into().unwrap()) as usize)
}

fn parse_binary(bytes: &[u8]) -> Result<StlGeometry, StlError> {
    let count = read_facet_count(bytes)
        .ok_or_else(|| StlError::Parse("unexpected end of binary STL data".to_string()))?;
    let mut vertices = Vec::with_capacity(count * 3);

    let mut offset = BINARY_HEADER_LEN + 4;
    for _ in 0..count {
        offset += 12; // skip facet normal
        for _ in 0..3 {
            let x = read_f32(bytes, offset)?;
            let y = read_f32(bytes, offset + 4)?;
            let z = read_f32(bytes, offset + 8)?;
            vertices.push([x as f64, y as f64, z as f64]);
            offset += 12;
        }
        offset += 2; // skip attribute byte count
    }

    Ok((vertices, triangles_for(count)))
}

fn read_f32(bytes: &[u8], offset: usize) -> Result<f32, StlError> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| StlError::Parse("unexpected end of binary STL data".to_string()))?;
    Ok(f32::from_le_bytes(slice.try_into().unwrap()))
}

fn parse_ascii(text: &str) -> Result<StlGeometry, StlError> {
    let mut vertices = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("vertex") else {
            continue;
        };
        let coords: Vec<f64> = rest
            .split_whitespace()
            .map(|v| v.parse::<f64>())
            .collect::<Result<_, _>>()
            .map_err(|_| StlError::Parse(format!("invalid vertex coordinates: {line}")))?;
        if coords.len() != 3 {
            return Err(StlError::Parse(format!(
                "invalid vertex coordinates: {line}"
            )));
        }
        vertices.push([coords[0], coords[1], coords[2]]);
    }

    if vertices.len() % 3 != 0 {
        return Err(StlError::Parse(
            "vertex count is not a multiple of 3 (malformed facets)".to_string(),
        ));
    }

    let triangles = triangles_for(vertices.len() / 3);
    Ok((vertices, triangles))
}

fn triangles_for(triangle_count: usize) -> Vec<[u32; 3]> {
    (0..triangle_count)
        .map(|i| {
            let base = (i * 3) as u32;
            [base, base + 1, base + 2]
        })
        .collect()
}
