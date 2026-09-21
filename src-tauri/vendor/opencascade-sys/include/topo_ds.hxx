#include <TopTools_ListOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Builder.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <bindings_common.hxx>

// `TopoDS::Vertex(shape)` etc. are static-method calls on OCCT < 7.9 (where
// TopoDS is a class) and free-function calls in the TopoDS namespace on OCCT
// >= 7.9 (see VENDORING.md, "Aenderung 1") - the CALL SYNTAX is identical in
// both cases, only cxx's `#[namespace = "TopoDS"]` binding mechanism cannot
// target a class, only a genuine namespace (it emits its own
// `namespace TopoDS { ... }` block, which collides with `class TopoDS` on
// pre-7.9 OCCT). These free-function shims sidestep that entirely: they are
// NOT inside any TopoDS scope themselves, so cxx binds them as ordinary free
// functions, and they simply forward to `TopoDS::X(...)` - valid C++ either
// way, making this genuinely version-independent across OCCT 7.8 and 7.9.
inline const TopoDS_Vertex &topods_cast_vertex(const TopoDS_Shape &shape) {
  return TopoDS::Vertex(shape);
}
inline const TopoDS_Edge &topods_cast_edge(const TopoDS_Shape &shape) {
  return TopoDS::Edge(shape);
}
inline const TopoDS_Wire &topods_cast_wire(const TopoDS_Shape &shape) {
  return TopoDS::Wire(shape);
}
inline const TopoDS_Face &topods_cast_face(const TopoDS_Shape &shape) {
  return TopoDS::Face(shape);
}
inline const TopoDS_Shell &topods_cast_shell(const TopoDS_Shape &shape) {
  return TopoDS::Shell(shape);
}
inline const TopoDS_Solid &topods_cast_solid(const TopoDS_Shape &shape) {
  return TopoDS::Solid(shape);
}
inline const TopoDS_Compound &topods_cast_compound(const TopoDS_Shape &shape) {
  return TopoDS::Compound(shape);
}
