#include <GCE2d_MakeSegment.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GC_MakeSegment.hxx>
#include <bindings_common.hxx>

inline std::unique_ptr<Handle_Geom_TrimmedCurve> GC_MakeSegment_Value(const GC_MakeSegment &segment) {
  return std::unique_ptr<Handle_Geom_TrimmedCurve>(new Handle_Geom_TrimmedCurve(segment.Value()));
}

inline std::unique_ptr<Handle_Geom2d_TrimmedCurve> GCE2d_MakeSegment_point_point(const gp_Pnt2d &p1,
                                                                                 const gp_Pnt2d &p2) {
  // GCE2d_MakeSegment returns a CONST opencascade::handle<T> - passed straight
  // to the Handle_T constructor, overload resolution fails on MSVC (the move
  // ctor rejects a const rvalue, deduction for the const& ctor doesn't
  // reliably get there). The detour via a non-const local variable turns it
  // into a plain lvalue that resolves unambiguously - on both compilers.
  opencascade::handle<Geom2d_TrimmedCurve> result = GCE2d_MakeSegment(p1, p2);
  return std::unique_ptr<Handle_Geom2d_TrimmedCurve>(new Handle_Geom2d_TrimmedCurve(result));
}

inline std::unique_ptr<Handle_Geom_TrimmedCurve> GC_MakeArcOfCircle_Value(const GC_MakeArcOfCircle &arc) {
  return std::unique_ptr<Handle_Geom_TrimmedCurve>(new Handle_Geom_TrimmedCurve(arc.Value()));
}
