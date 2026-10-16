import { isReadOnly } from './readOnly';
import { messageOf } from './errors';

let error = '';
const listeners = new Set<() => void>();
export const subscribeMetadataErrors = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
export const getMetadataError = () => error;
export function reportMetadataError(reason: unknown) {
  if (!isReadOnly()) return;
  error = messageOf(reason);
  listeners.forEach(listener => listener());
}
export function clearMetadataError() { error = ''; listeners.forEach(listener => listener()); }
