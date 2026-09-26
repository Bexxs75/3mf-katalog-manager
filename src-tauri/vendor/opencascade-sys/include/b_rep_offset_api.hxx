#include <BRepOffsetAPI_MakeOffset.hxx>
#include <BRepOffsetAPI_MakePipe.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <Law_Function.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopoDS_Shape.hxx>
#include <bindings_common.hxx>

// BRepOffsetAPI_MakePipeShell::SetLaw internally takes Handle(Law_Function)&
// (opencascade::handle<T>&); our Handle_Law_Function is a separate class derived
// from it on MSVC (see top_tools.hxx). cxx binds instance methods via a
// pointer-to-member comparison that needs exactly matching signatures -
// thin wrapper with our own, exact signature.
inline void BRepOffsetAPI_MakePipeShell_SetLaw(BRepOffsetAPI_MakePipeShell &pipe_shell, const TopoDS_Shape &profile,
                                               const Handle_Law_Function &law, bool with_contact,
                                               bool with_correction) {
  pipe_shell.SetLaw(profile, law, with_contact, with_correction);
}
