/**
 * Error message from a rejected promise. Tauri commands reject with a
 * `CmdError` object (`{ message, expected }`) or, in a few older spots, a
 * plain string; `Error` objects also have a string `message`. Without this
 * safeguard, `String(e)` would show "[object Object]" for a `CmdError`.
 */
export function messageOf(e: unknown): string {
  if (typeof e === 'object' && e !== null && typeof (e as { message?: unknown }).message === 'string') {
    return (e as { message: string }).message;
  }
  return String(e);
}
