/** The server expects a base name; the extension (incl. compound .gcode.3mf) stays fixed. */
export function webFileNameParts(name: string) {
  const dot = name.toLowerCase().endsWith('.gcode.3mf') ? name.length - 10 : name.lastIndexOf('.');
  return dot > 0 ? { base: name.slice(0, dot), extension: name.slice(dot) } : { base: name, extension: '' };
}
