/** Expose bidi controls so names cannot visually disguise their extension or path.
 * Use only for presentation; filesystem operations and editable values need the original.
 */
/** Windows verbatim prefix as shown for canonicalized paths: `\\?\C:\x` -> `C:\x`, `\\?\UNC\srv\share` -> `\\srv\share`. Presentation only. */
export function stripVerbatimPrefix(value: string): string {
  if (/^\\\\\?\\UNC\\/i.test(value)) return `\\\\${value.slice(8)}`;
  if (/^\\\\\?\\[A-Za-z]:/.test(value)) return value.slice(4);
  return value;
}

export function displayName(value: string): string {
  return stripVerbatimPrefix(value).replace(/[\u061C\u200E\u200F\u202A-\u202E\u2066-\u2069]/g,
    character => `[U+${character.charCodeAt(0).toString(16).toUpperCase().padStart(4, '0')}]`);
}
