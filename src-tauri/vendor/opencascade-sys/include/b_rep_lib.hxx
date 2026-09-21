#include <BRepLib.hxx>
#include <BRepLib_ToolTriangulatedShape.hxx>
#include <TopoDS_Shape.hxx>
#include <bindings_common.hxx>

// BRepLib_ToolTriangulatedShape::ComputeNormals nimmt intern Handle(Poly_Triangulation)&
// (opencascade::handle<T>&) entgegen; unser Handle_Poly_Triangulation ist auf MSVC eine
// davon abgeleitete, eigene Klasse (siehe top_tools.hxx). Direkte Methodenbindung
// scheitert am exakten Funktionszeiger-Vergleich von cxx - duenner Wrapper mit
// unserer eigenen, exakten Signatur.
inline void BRepLib_ToolTriangulatedShape_ComputeNormals(const TopoDS_Face &face,
                                                          const Handle_Poly_Triangulation &triangulation) {
  BRepLib_ToolTriangulatedShape::ComputeNormals(face, triangulation);
}
