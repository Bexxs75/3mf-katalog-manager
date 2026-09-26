pub use inner::*;

#[cxx::bridge]
mod inner {
    unsafe extern "C++" {
        include!("opencascade-sys/include/topo_ds.hxx");

        type TopLoc_Location = crate::top_loc::TopLoc_Location;
        type TopAbs_ShapeEnum = crate::top_abs::TopAbs_ShapeEnum;
        type TopAbs_Orientation = crate::top_abs::TopAbs_Orientation;
        type TopTools_ListOfShape = crate::top_tools::TopTools_ListOfShape;

        #[cxx_name = "topods_cast_vertex"]
        pub fn topods_vertex(shape: &TopoDS_Shape) -> &TopoDS_Vertex;
        #[cxx_name = "topods_cast_edge"]
        pub fn topods_edge(shape: &TopoDS_Shape) -> &TopoDS_Edge;
        #[cxx_name = "topods_cast_wire"]
        pub fn topods_wire(shape: &TopoDS_Shape) -> &TopoDS_Wire;
        #[cxx_name = "topods_cast_face"]
        pub fn topods_face(shape: &TopoDS_Shape) -> &TopoDS_Face;
        #[cxx_name = "topods_cast_shell"]
        pub fn topods_shell(shape: &TopoDS_Shape) -> &TopoDS_Shell;
        #[cxx_name = "topods_cast_solid"]
        pub fn topods_solid(shape: &TopoDS_Shape) -> &TopoDS_Solid;
        #[cxx_name = "topods_cast_compound"]
        pub fn topods_compound(shape: &TopoDS_Shape) -> &TopoDS_Compound;

        type TopoDS_Vertex;
        #[cxx_name = "upcast_ref"]
        pub fn cast_vertex_to_shape(wire: &TopoDS_Vertex) -> &TopoDS_Shape;
        #[cxx_name = "construct_unique"]
        pub fn TopoDS_Vertex_to_owned(shape: &TopoDS_Vertex) -> UniquePtr<TopoDS_Vertex>;

        type TopoDS_Edge;
        #[cxx_name = "upcast_ref"]
        pub fn cast_edge_to_shape(wire: &TopoDS_Edge) -> &TopoDS_Shape;
        #[cxx_name = "construct_unique"]
        pub fn TopoDS_Edge_to_owned(shape: &TopoDS_Edge) -> UniquePtr<TopoDS_Edge>;

        type TopoDS_Wire;
        #[cxx_name = "upcast_ref"]
        pub fn cast_wire_to_shape(wire: &TopoDS_Wire) -> &TopoDS_Shape;
        #[cxx_name = "construct_unique"]
        pub fn TopoDS_Wire_to_owned(shape: &TopoDS_Wire) -> UniquePtr<TopoDS_Wire>;

        type TopoDS_Face;
        #[cxx_name = "upcast_ref"]
        pub fn cast_face_to_shape(wire: &TopoDS_Face) -> &TopoDS_Shape;
        #[cxx_name = "construct_unique"]
        pub fn TopoDS_Face_new() -> UniquePtr<TopoDS_Face>;
        #[cxx_name = "construct_unique"]
        pub fn TopoDS_Face_to_owned(shape: &TopoDS_Face) -> UniquePtr<TopoDS_Face>;
        pub fn Orientation(self: &TopoDS_Face) -> TopAbs_Orientation;

        type TopoDS_Shell;
        #[cxx_name = "upcast_ref"]
        pub fn cast_shell_to_shape(wire: &TopoDS_Shell) -> &TopoDS_Shape;
        #[cxx_name = "construct_unique"]
        pub fn TopoDS_Shell_new() -> UniquePtr<TopoDS_Shell>;
        #[cxx_name = "construct_unique"]
        pub fn TopoDS_Shell_to_owned(shape: &TopoDS_Shell) -> UniquePtr<TopoDS_Shell>;
        #[cxx_name = "upcast"]
        pub fn TopoDS_Shell_as_shape(shell: UniquePtr<TopoDS_Shell>) -> UniquePtr<TopoDS_Shape>;

        type TopoDS_Solid;
        #[cxx_name = "upcast_ref"]
        pub fn cast_solid_to_shape(wire: &TopoDS_Solid) -> &TopoDS_Shape;
        #[cxx_name = "construct_unique"]
        pub fn TopoDS_Solid_to_owned(shape: &TopoDS_Solid) -> UniquePtr<TopoDS_Solid>;

        type TopoDS_Shape;
        #[cxx_name = "construct_unique"]
        pub fn TopoDS_Shape_to_owned(shape: &TopoDS_Shape) -> UniquePtr<TopoDS_Shape>;
        #[cxx_name = "Move"]
        pub fn translate(
            self: Pin<&mut TopoDS_Shape>,
            position: &TopLoc_Location,
            raise_exception: bool,
        );
        #[cxx_name = "Location"]
        pub fn set_global_translation(
            self: Pin<&mut TopoDS_Shape>,
            translation: &TopLoc_Location,
            raise_exception: bool,
        );
        pub fn IsNull(self: &TopoDS_Shape) -> bool;
        pub fn IsEqual(self: &TopoDS_Shape, other: &TopoDS_Shape) -> bool;
        pub fn ShapeType(self: &TopoDS_Shape) -> TopAbs_ShapeEnum;
        pub fn Orientation(self: &TopoDS_Shape) -> TopAbs_Orientation;

        type TopoDS_Compound;
        #[cxx_name = "upcast_ref"]
        pub fn cast_compound_to_shape(wire: &TopoDS_Compound) -> &TopoDS_Shape;
        #[cxx_name = "construct_unique"]
        pub fn TopoDS_Compound_new() -> UniquePtr<TopoDS_Compound>;
        #[cxx_name = "construct_unique"]
        pub fn TopoDS_Compound_to_owned(shape: &TopoDS_Compound) -> UniquePtr<TopoDS_Compound>;
        #[cxx_name = "upcast"]
        pub fn TopoDS_Compound_as_shape(
            compound: UniquePtr<TopoDS_Compound>,
        ) -> UniquePtr<TopoDS_Shape>;

        type TopoDS_Builder;
        pub fn MakeCompound(self: &TopoDS_Builder, compound: Pin<&mut TopoDS_Compound>);
        pub fn MakeShell(self: &TopoDS_Builder, compound: Pin<&mut TopoDS_Shell>);
        pub fn Add(self: &TopoDS_Builder, shape: Pin<&mut TopoDS_Shape>, compound: &TopoDS_Shape);

        // This is dumb:
        // https://cxx.rs/extern-c++.html#explicit-shim-trait-impls
        #[cxx_name = "list_to_vector"]
        pub fn shape_list_to_vector(
            list: &TopTools_ListOfShape,
        ) -> UniquePtr<CxxVector<TopoDS_Shape>>;
    }
}

unsafe impl Send for inner::TopoDS_Edge {}
unsafe impl Send for inner::TopoDS_Wire {}
unsafe impl Send for inner::TopoDS_Face {}
unsafe impl Send for inner::TopoDS_Shell {}
unsafe impl Send for inner::TopoDS_Solid {}
unsafe impl Send for inner::TopoDS_Compound {}
unsafe impl Send for inner::TopoDS_Shape {}

/// OCCT 7.9 turned `TopoDS` from a class into a namespace; cxx can form
/// neither a type alias nor a `#[namespace = "TopoDS"]` binding that covers
/// both OCCT versions at once (see the `topods_cast_*` shims in
/// topo_ds.hxx, which solve this version-independently). The cast functions
/// therefore call these shims and are offered here under the familiar path
/// `TopoDS::Face(...)`. Patch against upstream 0.3.0 - see VENDORING.md.
pub struct TopoDS;

#[allow(non_snake_case)]
impl TopoDS {
    pub fn Vertex(shape: &TopoDS_Shape) -> &TopoDS_Vertex {
        inner::topods_vertex(shape)
    }

    pub fn Edge(shape: &TopoDS_Shape) -> &TopoDS_Edge {
        inner::topods_edge(shape)
    }

    pub fn Wire(shape: &TopoDS_Shape) -> &TopoDS_Wire {
        inner::topods_wire(shape)
    }

    pub fn Face(shape: &TopoDS_Shape) -> &TopoDS_Face {
        inner::topods_face(shape)
    }

    pub fn Shell(shape: &TopoDS_Shape) -> &TopoDS_Shell {
        inner::topods_shell(shape)
    }

    pub fn Solid(shape: &TopoDS_Shape) -> &TopoDS_Solid {
        inner::topods_solid(shape)
    }

    pub fn Compound(shape: &TopoDS_Shape) -> &TopoDS_Compound {
        inner::topods_compound(shape)
    }
}
