#include <TopTools_HSequenceOfShape.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopoDS_Face.hxx>
#include <bindings_common.hxx>

inline std::unique_ptr<Handle_TopTools_HSequenceOfShape> new_Handle_TopTools_HSequenceOfShape() {
  // Builds via the pointer constructor of Handle_TopTools_HSequenceOfShape
  // itself instead of a separately allocated opencascade::handle<T>*:
  // on MSVC Handle_TopTools_HSequenceOfShape is a class DERIVED from
  // opencascade::handle<T> (for C++/CLI compatibility, see
  // Standard_Handle.hxx), on other compilers just a type alias.
  // A handle<T>* can't be implicitly converted into a Handle_T* (derived
  // type) - the direct pointer constructor, however, works identically on
  // both variants.
  auto sequence = new TopTools_HSequenceOfShape();
  return std::unique_ptr<Handle_TopTools_HSequenceOfShape>(new Handle_TopTools_HSequenceOfShape(sequence));
}

// Own, non-templated deref function instead of the generic
// `handle_try_deref<T>` template from bindings_common.hxx: for the
// Result<&T> return, cxx generates a function pointer comparison that needs
// exactly matching signatures. The template is parameterized on
// `opencascade::handle<T>`, but MSVC's Handle_TopTools_HSequenceOfShape is a
// separate type derived from it - the pointer types then no longer match
// exactly (no covariance for plain function pointers).
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
