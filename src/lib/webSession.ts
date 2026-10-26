import { useSyncExternalStore } from 'react';
import { isReadOnly } from './readOnly';
/** Browser session and CSRF token live only in memory. */
export interface WebSession { authenticated: boolean; mustChangePassword: boolean; csrfToken: string | null; writable?: boolean; uploadable?: boolean; fileOps?: boolean; slicerLinks?: boolean }
let session: WebSession = { authenticated: false, mustChangePassword: false, csrfToken: null };
let sessionEpoch = 0;
const subscribers = new Set<() => void>();
export const getSnapshot = () => session;
export const subscribe = (callback: () => void) => { subscribers.add(callback); return () => { subscribers.delete(callback); }; };
function publish(next: WebSession) { session = { ...next, ...(next.fileOps !== undefined ? { fileOps: next.authenticated && !next.mustChangePassword && next.writable === true && next.fileOps === true } : {}), writable: next.authenticated && !next.mustChangePassword && next.writable === true }; subscribers.forEach(callback => callback()); }
export function authenticationFailed(status: number, code?: string) {
  if (status === 401 || (status === 403 && code === 'password_change_required')) sessionEpoch++;
  if (status === 401) publish({ authenticated: false, mustChangePassword: false, csrfToken: null });
  else if (status === 403 && code === 'password_change_required') publish({ ...session, mustChangePassword: true, writable: false });
}
export async function readResponse<T>(response: Response): Promise<T> {
  if (!response.ok) {
    if (response.status === 401) authenticationFailed(401);
    const error = await response.json();
    authenticationFailed(response.status, error.code);
    if (error.kind === 'restart_required') blockFileOperations();
    if (error.kind === 'fileops_disabled') publish({ ...session, fileOps: false });
    const retryAfter = response.headers.get('Retry-After');
    throw retryAfter ? { ...error, retryAfter } : error;
  }
  return (response.headers.get('Content-Type')?.split(';')[0] === 'application/octet-stream'
    ? response.arrayBuffer() : response.json()) as Promise<T>;
}
async function request(path: string, args?: Record<string, unknown>) {
  return readResponse<WebSession>(await fetch('/api/' + path, {
    method: args === undefined ? 'GET' : 'POST', credentials: 'same-origin',
    ...(args === undefined ? {} : { headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': session.csrfToken ?? '' }, body: JSON.stringify(args) }),
  }));
}
export async function getSession() {
  const epoch = ++sessionEpoch;
  const next = await request('session');
  if (epoch === sessionEpoch) publish(next);
  return session;
}
export async function login(password: string) {
  const epoch = ++sessionEpoch;
  const next = await request('login', { password });
  if (epoch === sessionEpoch) publish({ ...next, authenticated: true });
}
export async function logout() {
  ++sessionEpoch;
  const pending = request('logout', {});
  publish({ authenticated: false, mustChangePassword: false, csrfToken: null });
  await pending;
}
export async function changePassword(current: string, next: string) {
  const epoch = ++sessionEpoch;
  const result = await request('password', { current, new: next });
  if (epoch === sessionEpoch) publish({ ...result, authenticated: true, mustChangePassword: false });
}

export const canEditMetadata = () => !isReadOnly() || session.writable === true;
export const useCanEditMetadata = () => useSyncExternalStore(subscribe, canEditMetadata, canEditMetadata);

/** Browser upload is a separate server capability; filesystem actions stay locked. */
export const canUpload = () => isReadOnly() && session.authenticated && !session.mustChangePassword && session.writable === true && session.uploadable === true;
export const useCanUpload = () => useSyncExternalStore(subscribe, canUpload, canUpload);

/** Signed slicer links are a separate, opt-in server capability and need no write access. */
export const canOpenInSlicerLink = () => isReadOnly() && session.authenticated && !session.mustChangePassword && session.slicerLinks === true;
export const useCanOpenInSlicerLink = () => useSyncExternalStore(subscribe, canOpenInSlicerLink, canOpenInSlicerLink);

/** Snapshot persistence has a separate capability from filesystem writes. */
export const canSaveSnapshots = () => import.meta.env.VITE_TARGET === 'web' ? canEditMetadata() : !isReadOnly();

/** File operations are independently enabled by the server. */
export const canManageFiles = () => !isReadOnly() || (session.authenticated && !session.mustChangePassword && session.writable === true && session.fileOps === true);
export const useCanManageFiles = () => useSyncExternalStore(subscribe, canManageFiles, canManageFiles);

export function blockFileOperations() { sessionEpoch++; publish({ ...session, fileOps: false }); }
