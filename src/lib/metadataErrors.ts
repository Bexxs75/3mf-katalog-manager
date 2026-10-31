import { isReadOnly } from './readOnly';
import { messageOf } from './errors';

let error = '';
let plain = false;
const listeners = new Set<() => void>();
export const subscribeMetadataErrors = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
export const getMetadataError = () => error;
/** True when the message already explains itself (a known refusal), so no generic "reload" framing is added. */
export const isMetadataErrorPlain = () => plain;
export function reportMetadataError(reason: unknown) {
  if (!isReadOnly()) return;
  error = messageOf(reason);
  plain = typeof reason === 'object' && reason !== null && (reason as { plain?: unknown }).plain === true;
  listeners.forEach(listener => listener());
}
export function clearMetadataError() { error = ''; plain = false; listeners.forEach(listener => listener()); }
