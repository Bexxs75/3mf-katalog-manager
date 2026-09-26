/**
 * Splits a file name into base and extension (including the dot) for the
 * rename UI, which shows the extension as non-editable. The separator is the
 * LAST dot; a dot at position 0 (".gitignore") doesn't count.
 */
export function splitFileName(name: string): { base: string; extension: string } {
  const dotIndex = name.lastIndexOf('.');
  if (dotIndex <= 0) return { base: name, extension: '' };
  return { base: name.slice(0, dotIndex), extension: name.slice(dotIndex) };
}
