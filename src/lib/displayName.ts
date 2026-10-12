/** Expose bidi controls so names cannot visually disguise their extension or path.
 * Use only for presentation; filesystem operations and editable values need the original.
 */
export function displayName(value: string): string {
  return value.replace(/[\u061C\u200E\u200F\u202A-\u202E\u2066-\u2069]/g,
    character => `[U+${character.charCodeAt(0).toString(16).toUpperCase().padStart(4, '0')}]`);
}
