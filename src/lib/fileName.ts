/**
 * Trennt einen Dateinamen in Basis und Endung (samt Punkt) fuer die
 * Umbenennen-UI, die die Endung nicht editierbar zeigt. Trenner ist der LETZTE
 * Punkt; ein Punkt an Position 0 (".gitignore") zaehlt nicht.
 */
export function splitFileName(name: string): { base: string; extension: string } {
  const dotIndex = name.lastIndexOf('.');
  if (dotIndex <= 0) return { base: name, extension: '' };
  return { base: name.slice(0, dotIndex), extension: name.slice(dotIndex) };
}
