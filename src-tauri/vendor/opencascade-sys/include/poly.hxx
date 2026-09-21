#include <Poly_Connect.hxx>
#include <Poly_Triangulation.hxx>
#include <bindings_common.hxx>

inline std::unique_ptr<Handle_Poly_Triangulation>
Handle_Poly_Triangulation_new(std::unique_ptr<Poly_Triangulation> triangulation) {
  return std::unique_ptr<Handle_Poly_Triangulation>(new Handle_Poly_Triangulation(triangulation.release()));
}

// Eigene, nicht-templatisierte Deref-Funktion statt des generischen
// `handle_try_deref<T>`-Templates aus bindings_common.hxx - siehe
// top_tools.hxx fuer die ausfuehrliche Begruendung (MSVC-Handle_X ist eine
// abgeleitete Klasse, kein Typalias; Funktionszeiger-Vergleich braucht
// exakte Signaturgleichheit).
inline const Poly_Triangulation &poly_triangulation_handle_try_deref(const Handle_Poly_Triangulation &handle) {
  if (handle.IsNull()) {
    throw std::runtime_error("null handle dereference");
  }
  return *handle;
}

inline std::unique_ptr<gp_Dir> Poly_Triangulation_Normal(const Poly_Triangulation &triangulation,
                                                         const Standard_Integer index) {
  return std::unique_ptr<gp_Dir>(new gp_Dir(triangulation.Normal(index)));
}

inline std::unique_ptr<gp_Pnt> Poly_Triangulation_Node(const Poly_Triangulation &triangulation,
                                                       const Standard_Integer index) {
  return std::unique_ptr<gp_Pnt>(new gp_Pnt(triangulation.Node(index)));
}

inline std::unique_ptr<gp_Pnt2d> Poly_Triangulation_UV(const Poly_Triangulation &triangulation,
                                                       const Standard_Integer index) {
  return std::unique_ptr<gp_Pnt2d>(new gp_Pnt2d(triangulation.UVNode(index)));
}
