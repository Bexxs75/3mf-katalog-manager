#include <ShapeAnalysis.hxx>
#include <ShapeAnalysis_FreeBounds.hxx>
#include <TopTools_HSequenceOfShape.hxx>
#include <bindings_common.hxx>

// ShapeAnalysis_FreeBounds::ConnectEdgesToWires nimmt intern
// Handle(TopTools_HSequenceOfShape)& (opencascade::handle<T>&) entgegen; unser
// Handle_TopTools_HSequenceOfShape ist auf MSVC eine davon abgeleitete, eigene
// Klasse (siehe top_tools.hxx). Direkte Methodenbindung scheitert am exakten
// Funktionszeiger-Vergleich von cxx - duenner Wrapper mit unserer eigenen,
// exakten Signatur.
inline void ShapeAnalysis_FreeBounds_ConnectEdgesToWires(Handle_TopTools_HSequenceOfShape &edges,
                                                          Standard_Real tolerance, Standard_Boolean shared,
                                                          Handle_TopTools_HSequenceOfShape &wires) {
  ShapeAnalysis_FreeBounds::ConnectEdgesToWires(edges, tolerance, shared, wires);
}
