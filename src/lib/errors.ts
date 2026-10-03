/** Error shown in the UI; `unexpected` decides whether "Report problem" is offered. */
export interface AppError {
  message: string;
  unexpected: boolean;
}

interface CmdError {
  message: string;
  expected: boolean;
}

function isCmdError(e: unknown): e is CmdError {
  return (
    typeof e === 'object' &&
    e !== null &&
    typeof (e as CmdError).message === 'string' &&
    typeof (e as CmdError).expected === 'boolean'
  );
}

/**
 * Error message from a rejected promise. Tauri commands reject with a
 * `CmdError` object (`{ message, expected }`) or, in a few older spots, a
 * plain string; `Error` objects also have a string `message`. Also falls
 * back to any string `message` property so other plain error-shaped
 * objects still read out their text instead of "[object Object]".
 */
export function messageOf(e: unknown): string {
  if (isCmdError(e)) return e.message;
  if (e instanceof Error) return e.message;
  if (typeof e === 'object' && e !== null && typeof (e as { message?: unknown }).message === 'string') {
    return (e as { message: string }).message;
  }
  return String(e);
}

/** Turns any rejection into an `AppError`; only a `CmdError` with `expected: true` counts as expected. */
export function toAppError(e: unknown): AppError {
  return { message: messageOf(e), unexpected: isCmdError(e) ? !e.expected : true };
}

/** Builds an expected error for frontend-side validation (no backend command involved). */
export function expectedError(message: string): AppError {
  return { message, unexpected: false };
}
