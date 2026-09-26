#include <BRepLib.hxx>
#include <BRepLib_ToolTriangulatedShape.hxx>
#include <TopoDS_Shape.hxx>
#include <bindings_common.hxx>

// BRepLib_ToolTriangulatedShape::ComputeNormals internally takes Handle(Poly_Triangulation)&
// (opencascade::handle<T>&); our Handle_Poly_Triangulation is a separate class derived
// from it on MSVC (see top_tools.hxx). Direct method binding fails on cxx's exact
// function pointer comparison - thin wrapper with our own, exact signature.
inline void BRepLib_ToolTriangulatedShape_ComputeNormals(const TopoDS_Face &face,
                                                          const Handle_Poly_Triangulation &triangulation) {
  BRepLib_ToolTriangulatedShape::ComputeNormals(face, triangulation);
}
