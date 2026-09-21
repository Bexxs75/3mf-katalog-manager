#include <BRepOffsetAPI_MakeOffset.hxx>
#include <BRepOffsetAPI_MakePipe.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <Law_Function.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopoDS_Shape.hxx>
#include <bindings_common.hxx>

// BRepOffsetAPI_MakePipeShell::SetLaw nimmt intern Handle(Law_Function)&
// (opencascade::handle<T>&) entgegen; unser Handle_Law_Function ist auf MSVC eine
// davon abgeleitete, eigene Klasse (siehe top_tools.hxx). cxx bindet Instanzmethoden
// ueber einen Pointer-to-member-Vergleich, der exakte Signaturgleichheit braucht -
// duenner Wrapper mit unserer eigenen, exakten Signatur.
inline void BRepOffsetAPI_MakePipeShell_SetLaw(BRepOffsetAPI_MakePipeShell &pipe_shell, const TopoDS_Shape &profile,
                                               const Handle_Law_Function &law, bool with_contact,
                                               bool with_correction) {
  pipe_shell.SetLaw(profile, law, with_contact, with_correction);
}
