//! Dimensions, volume and body count of a shape.

use opencascade_sys::{
    b_rep_bnd_lib::BRepBndLib, b_rep_g_prop::BRepGProp, bnd::Bnd_Box_new, g_prop::GProps_new,
    top_abs::TopAbs_ShapeEnum, top_exp::TopExp_Explorer_new, topo_ds::TopoDS_Shape,
};

use crate::geometry::BoundingBox;

/// Bounding box in millimeters. `use_triangulation = false`: the dimensions are
/// needed before tessellation and should be exact.
pub fn bounding_box(shape: &TopoDS_Shape) -> Option<BoundingBox> {
    let mut bnd = Bnd_Box_new();
    BRepBndLib::Add(shape, bnd.pin_mut(), false);
    if bnd.IsVoid() {
        return None;
    }

    let mut x_min = 0.0;
    let mut y_min = 0.0;
    let mut z_min = 0.0;
    let mut x_max = 0.0;
    let mut y_max = 0.0;
    let mut z_max = 0.0;
    bnd.Get(
        &mut x_min, &mut y_min, &mut z_min, &mut x_max, &mut y_max, &mut z_max,
    );

    let mut bb = BoundingBox::empty();
    bb.extend([x_min, y_min, z_min]);
    bb.extend([x_max, y_max, z_max]);
    bb.is_valid().then_some(bb)
}

/// Volume in cm^3 - same convention as the existing parsers: cubic millimeters
/// divided by 1000. Open geometry returns `None`.
pub fn volume_cm3(shape: &TopoDS_Shape) -> Option<f64> {
    let mut props = GProps_new();
    BRepGProp::VolumeProperties(shape, props.pin_mut(), true, false, false);

    let volume_mm3 = props.Mass();
    (volume_mm3 > 0.0).then_some(volume_mm3 / 1000.0)
}

/// Number of SOLID bodies. Without solids but with geometry, it counts as one
/// body; the assembly structure stays pure metadata.
pub fn body_count(shape: &TopoDS_Shape) -> usize {
    let mut solids = 0usize;
    let mut explorer = TopExp_Explorer_new(shape, TopAbs_ShapeEnum::TopAbs_SOLID);
    while explorer.More() {
        solids += 1;
        explorer.pin_mut().Next();
    }
    if solids > 0 {
        return solids;
    }

    let explorer = TopExp_Explorer_new(shape, TopAbs_ShapeEnum::TopAbs_FACE);
    usize::from(explorer.More())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    fn shape(name: &str) -> cxx::UniquePtr<TopoDS_Shape> {
        crate::step::reader::read_shape(&fixture(name)).expect("fixture must be readable")
    }

    #[test]
    fn cube_bounding_box_is_ten_millimetres_per_axis() {
        let bb = bounding_box(&shape("wuerfel-10mm.step")).expect("box");
        let size = bb.size();
        for value in size {
            assert!((value - 10.0).abs() < 1e-6, "unerwartet: {size:?}");
        }
    }

    #[test]
    fn cube_volume_is_one_cubic_centimetre() {
        let volume = volume_cm3(&shape("wuerfel-10mm.step")).expect("volume");
        assert!((volume - 1.0).abs() < 1e-6, "unerwartet: {volume}");
    }

    #[test]
    fn cube_counts_as_one_body() {
        assert_eq!(body_count(&shape("wuerfel-10mm.step")), 1);
    }

    #[test]
    fn the_two_body_fixture_counts_as_two_bodies() {
        assert_eq!(body_count(&shape("zwei-koerper.step")), 2);
    }

    #[test]
    fn the_two_body_fixture_spans_the_offset_between_both_bodies() {
        let bb = bounding_box(&shape("zwei-koerper.step")).expect("box");
        let size = bb.size();
        assert!((size[0] - 40.0).abs() < 1e-6, "X-Ausdehnung: {}", size[0]);
        assert!((size[1] - 10.0).abs() < 1e-6, "Y-Ausdehnung: {}", size[1]);
    }
}
