import { isReadOnly } from './readOnly';
import { toAppError, type AppError } from './errors';

/** What is shown, kept as the error's code and params (or technical text) so the text follows the language. */
export type MetadataErrorDetail = Pick<AppError, 'message' | 'params'>;

let error: MetadataErrorDetail | null = null;
let plain = false;
const listeners = new Set<() => void>();
export const subscribeMetadataErrors = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
/** The message of the standing error: a code or technical text, empty when there is none. */
export const getMetadataError = () => error?.message ?? '';
/** The standing error with its params, one stable object per report (for `useSyncExternalStore`). */
export const getMetadataErrorDetail = () => error;
/** True when the message already explains itself (a known refusal), so no generic "reload" framing is added. */
export const isMetadataErrorPlain = () => plain;
export function reportMetadataError(reason: unknown) {
  if (!isReadOnly()) return;
  const { message, params } = toAppError(reason);
  error = params ? { message, params } : { message };
  plain = typeof reason === 'object' && reason !== null && (reason as { plain?: unknown }).plain === true;
  listeners.forEach(listener => listener());
}
export function clearMetadataError() { error = null; plain = false; listeners.forEach(listener => listener()); }
