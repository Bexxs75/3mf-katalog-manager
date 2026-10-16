/** Browser session and CSRF token live only in memory. */
export interface WebSession { authenticated: boolean; mustChangePassword: boolean; csrfToken: string | null }
let session: WebSession = { authenticated: false, mustChangePassword: false, csrfToken: null };
const subscribers = new Set<() => void>();
export const getSnapshot = () => session;
export const subscribe = (callback: () => void) => { subscribers.add(callback); return () => { subscribers.delete(callback); }; };
function publish(next: WebSession) { session = next; subscribers.forEach(callback => callback()); }
export function authenticationFailed(status: number, code?: string) {
  if (status === 401) publish({ authenticated: false, mustChangePassword: false, csrfToken: null });
  else if (status === 403 && code === 'password_change_required') publish({ ...session, mustChangePassword: true });
}
export async function readResponse<T>(response: Response): Promise<T> {
  if (!response.ok) {
    const error = await response.json();
    authenticationFailed(response.status, error.code);
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
export async function getSession() { const next = await request('session'); publish(next); return next; }
export async function login(password: string) { const next = await request('login', { password }); publish({ ...next, authenticated: true }); }
export async function logout() { await request('logout', {}); publish({ authenticated: false, mustChangePassword: false, csrfToken: null }); }
export async function changePassword(current: string, next: string) { const result = await request('password', { current, new: next }); publish({ ...result, authenticated: true, mustChangePassword: false }); }
