#include <TopTools_HSequenceOfShape.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopoDS_Face.hxx>
#include <bindings_common.hxx>

inline std::unique_ptr<Handle_TopTools_HSequenceOfShape> new_Handle_TopTools_HSequenceOfShape() {
  // Baut ueber den Zeiger-Konstruktor von Handle_TopTools_HSequenceOfShape
  // selbst statt ueber einen separat allozierten opencascade::handle<T>*:
  // Auf MSVC ist Handle_TopTools_HSequenceOfShape eine von
  // opencascade::handle<T> ABGELEITETE Klasse (fuer C++/CLI-Kompatibilitaet,
  // siehe Standard_Handle.hxx), auf anderen Compilern nur ein Typalias.
  // Ein handle<T>* laesst sich nicht implizit in ein Handle_T* (abgeleiteter
  // Typ) verwandeln - der direkte Zeiger-Konstruktor funktioniert dagegen
  // identisch auf beiden Varianten.
  auto sequence = new TopTools_HSequenceOfShape();
  return std::unique_ptr<Handle_TopTools_HSequenceOfShape>(new Handle_TopTools_HSequenceOfShape(sequence));
}

// Eigene, nicht-templatisierte Deref-Funktion statt des generischen
// `handle_try_deref<T>`-Templates aus bindings_common.hxx: cxx erzeugt fuer
// die Result<&T>-Rueckgabe einen Funktionszeiger-Vergleich, der exakte
// Signaturgleichheit braucht. Das Template ist auf `opencascade::handle<T>`
// parametrisiert, MSVCs Handle_TopTools_HSequenceOfShape ist aber ein davon
// abgeleiteter, eigener Typ - die Zeigertypen passen dann nicht mehr exakt
// zusammen (kein Kovarianz bei reinen Funktionszeigern).
inline const TopTools_HSequenceOfShape &top_tools_handle_try_deref(const Handle_TopTools_HSequenceOfShape &handle) {
  if (handle.IsNull()) {
    throw std::runtime_error("null handle dereference");
  }
  return *handle;
}

inline void TopTools_HSequenceOfShape_append(Handle_TopTools_HSequenceOfShape &handle, const TopoDS_Shape &shape) {
  handle->Append(shape);
}

inline Standard_Integer TopTools_HSequenceOfShape_length(const Handle_TopTools_HSequenceOfShape &handle) {
  return handle->Length();
}

inline const TopoDS_Shape &TopTools_HSequenceOfShape_value(const Handle_TopTools_HSequenceOfShape &handle,
                                                           Standard_Integer index) {
  return handle->Value(index);
}
