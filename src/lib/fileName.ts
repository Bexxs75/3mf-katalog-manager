/**
 * Trennt einen Dateinamen in Basisname und Endung (samt Punkt), fuer die
 * Umbenennen-UI: die Endung wird dort bewusst nicht editierbar angezeigt,
 * damit ein Nutzer eine 3mf/STL-Datei nicht versehentlich per Umbenennen
 * ihrer Endung beraubt (die App erkennt/parst Modelle ueber den in der DB
 * gespeicherten `fileType`, nicht die Endung - eine verlorene Endung wuerde
 * die Datei aber z.B. fuer den externen Slicer/Datei-Manager unbrauchbar
 * wirken lassen).
 *
 * Nimmt den LETZTEN Punkt als Trenner (kein Spezialfall fuer doppelte
 * Endungen wie ".tar.gz" - fuer die von dieser App verwalteten Formate
 * (3mf/stl) gibt es das nicht). Ein Punkt an Position 0 (versteckte Datei
 * wie ".gitignore") zaehlt nicht als Endungstrenner, sonst waere der
 * gesamte Name eine "leere" Basis plus Endung.
 */
export function splitFileName(name: string): { base: string; extension: string } {
  const dotIndex = name.lastIndexOf('.');
  if (dotIndex <= 0) return { base: name, extension: '' };
  return { base: name.slice(0, dotIndex), extension: name.slice(dotIndex) };
}
