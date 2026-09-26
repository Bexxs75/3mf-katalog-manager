#include <ShapeAnalysis.hxx>
#include <ShapeAnalysis_FreeBounds.hxx>
#include <TopTools_HSequenceOfShape.hxx>
#include <bindings_common.hxx>

// ShapeAnalysis_FreeBounds::ConnectEdgesToWires internally takes
// Handle(TopTools_HSequenceOfShape)& (opencascade::handle<T>&); our
// Handle_TopTools_HSequenceOfShape is a separate class derived from it on
// MSVC (see top_tools.hxx). Direct method binding fails on cxx's exact
// function pointer comparison - thin wrapper with our own, exact signature.
inline void ShapeAnalysis_FreeBounds_ConnectEdgesToWires(Handle_TopTools_HSequenceOfShape &edges,
                                                          Standard_Real tolerance, Standard_Boolean shared,
                                                          Handle_TopTools_HSequenceOfShape &wires) {
  ShapeAnalysis_FreeBounds::ConnectEdgesToWires(edges, tolerance, shared, wires);
}
