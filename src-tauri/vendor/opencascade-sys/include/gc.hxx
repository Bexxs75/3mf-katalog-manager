#include <GCE2d_MakeSegment.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GC_MakeSegment.hxx>
#include <bindings_common.hxx>

inline std::unique_ptr<Handle_Geom_TrimmedCurve> GC_MakeSegment_Value(const GC_MakeSegment &segment) {
  return std::unique_ptr<Handle_Geom_TrimmedCurve>(new Handle_Geom_TrimmedCurve(segment.Value()));
}

inline std::unique_ptr<Handle_Geom2d_TrimmedCurve> GCE2d_MakeSegment_point_point(const gp_Pnt2d &p1,
                                                                                 const gp_Pnt2d &p2) {
  // GCE2d_MakeSegment gibt ein CONST opencascade::handle<T> zurueck - direkt
  // an den Handle_T-Konstruktor durchgereicht schlaegt die Ueberladungs-
  // aufloesung auf MSVC fehl (Move-Ctor lehnt const rvalue ab, Deduktion
  // fuer den const&-Ctor findet nicht zuverlaessig hin). Der Umweg ueber
  // eine nicht-konstante lokale Variable macht daraus ein gewoehnliches
  // Lvalue, das eindeutig aufloest - auf beiden Compilern.
  opencascade::handle<Geom2d_TrimmedCurve> result = GCE2d_MakeSegment(p1, p2);
  return std::unique_ptr<Handle_Geom2d_TrimmedCurve>(new Handle_Geom2d_TrimmedCurve(result));
}

inline std::unique_ptr<Handle_Geom_TrimmedCurve> GC_MakeArcOfCircle_Value(const GC_MakeArcOfCircle &arc) {
  return std::unique_ptr<Handle_Geom_TrimmedCurve>(new Handle_Geom_TrimmedCurve(arc.Value()));
}
