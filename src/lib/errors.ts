/**
 * Error message from a rejected promise. Tauri commands often reject with
 * a plain string instead of an `Error` object, hence no `e.message`
 * without this safeguard.
 */
export function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
