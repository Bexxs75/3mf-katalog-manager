import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { authenticationFailed, canManageFiles, login } from './webSession';
import { emptyWebTrash, fileOp, fileOpsBatch, subscribeFileOpsApplied, subscribeFileOpsRefresh } from './fileOps';
import { clearMetadataError, getMetadataError, subscribeMetadataErrors } from './metadataErrors';
import { fileOpsError, retryAfterSeconds } from './fileOpsErrors';
let calls: { command: string; args: Record<string, unknown> }[];
let respond: (command: string) => Response;
beforeEach(async () => {
  calls = []; respond = () => Response.json(null); clearMetadataError();
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    if (!url.includes('/invoke/')) return Response.json({ authenticated: true, writable: true, fileOps: true, csrfToken: 'token' });
    const command = url.split('/').pop()!; calls.push({ command, args: JSON.parse(String(options?.body)) }); return respond(command);
  }));
  await login('password');
});
afterEach(() => { authenticationFailed(401); vi.useRealTimers(); vi.unstubAllGlobals(); });
const limited = (seconds: string, status = 429) => Response.json({ kind: 'rate_limited' }, { status, headers: { 'Retry-After': seconds } });

// Befund 1: Wartezeit begrenzen
it('retries once automatically when Retry-After is 3 seconds', async () => {
  vi.useFakeTimers(); respond = () => calls.length === 1 ? limited('3') : Response.json(null);
  const pending = fileOp('rename_file', { fileId: '1', name: 'new' });
  await vi.advanceTimersByTimeAsync(2999); expect(calls).toHaveLength(1);
  await vi.advanceTimersByTimeAsync(1); await pending; expect(calls).toHaveLength(2);
});
it('retries the busy write gate at most once, even if the answer is still busy', async () => {
  vi.useFakeTimers(); respond = () => Response.json({ message: 'Schreibzugriff derzeit ausgelastet' }, { status: 503, headers: { 'Retry-After': '2' } });
  const rejected = expect(fileOp('rename_file', { fileId: '1', name: 'new' })).rejects.toMatchObject({ message: expect.stringContaining('2 Sekunden') });
  await vi.runAllTimersAsync(); await rejected; expect(calls).toHaveLength(2);
});
it('aborts immediately on a 60 second Retry-After and names the wait', async () => {
  vi.useFakeTimers(); respond = () => limited('60');
  const rejected = expect(fileOp('delete_files', { fileIds: ['1'] })).rejects.toMatchObject({ message: expect.stringMatching(/Bitte in 1 Minute erneut versuchen/) });
  await vi.advanceTimersByTimeAsync(0); await rejected;
  expect(vi.getTimerCount()).toBe(0);
  await vi.advanceTimersByTimeAsync(3_600_000); expect(calls).toHaveLength(1);
});
it('names minutes for longer waits and parses HTTP dates', () => {
  expect(fileOpsError({ kind: 'rate_limited', retryAfter: '900' }).message).toContain('15 Minuten');
  expect(retryAfterSeconds('abc')).toBeNull(); expect(retryAfterSeconds('')).toBeNull(); expect(retryAfterSeconds('7')).toBe(7);
  expect(retryAfterSeconds(new Date(Date.now() + 120_000).toUTCString())).toBeGreaterThan(100);
});
it('stops the trash loop on a long pause, reports progress and never resumes with the old token', async () => {
  vi.useFakeTimers();
  respond = command => command === 'empty_trash_start' ? Response.json({ token: 'snap', remaining: 400 })
    : calls.filter(c => c.command === 'empty_trash_block').length === 1 ? Response.json({ results: [{ id: '1', ok: true }], remaining: 200 }) : limited('1800');
  const progress = vi.fn();
  const rejected = expect(emptyWebTrash({ count: 400, bytes: 1, maxDeletedAt: null }, 'secret', progress)).rejects.toMatchObject({ message: expect.stringMatching(/200 von 400 gelöscht[\s\S]*Pause nötig[\s\S]*30 Minuten/) });
  await vi.runAllTimersAsync(); await rejected;
  const count = calls.length; expect(calls.map(c => c.command)).toEqual(['empty_trash_start', 'empty_trash_block', 'empty_trash_block']);
  await vi.advanceTimersByTimeAsync(2 * 3_600_000); expect(calls).toHaveLength(count);
  expect(getMetadataError()).toContain('Pause nötig');
});
it('cancels the trash loop between blocks without touching the running block', async () => {
  vi.useFakeTimers(); let stop = false;
  respond = command => command === 'empty_trash_start' ? Response.json({ token: 'snap', remaining: 3 }) : (stop = true, Response.json({ results: [{ id: '1', ok: true }], remaining: 2 }));
  const rejected = expect(emptyWebTrash({ count: 3, bytes: 0, maxDeletedAt: null }, 'secret', vi.fn(), () => stop)).rejects.toMatchObject({ kind: 'cancelled', message: expect.stringContaining('1 von 3 gelöscht') });
  await vi.runAllTimersAsync(); await rejected;
  expect(calls.map(c => c.command)).toEqual(['empty_trash_start', 'empty_trash_block']);
});
it('cancels a batch between chunks and lists the unprocessed IDs', async () => {
  let stop = false; respond = () => (stop = true, Response.json([{ id: '1', ok: true }]));
  const ids = Array.from({ length: 450 }, (_, i) => String(i + 1));
  await expect(fileOpsBatch('delete_files', ids, {}, () => stop)).rejects.toMatchObject({ message: expect.stringMatching(/Abgebrochen[\s\S]*201:[\s\S]*450:/) });
  expect(calls).toHaveLength(1);
});

// Befund 3: Antwortform
it('treats a single restart_required object on HTTP 200 as a lock', async () => {
  respond = () => Response.json({ kind: 'restart_required', opIds: ['abc'] });
  await expect(fileOp('rename_file', { fileId: '1', name: 'x' })).rejects.toMatchObject({ message: expect.stringContaining('abc') });
  expect(canManageFiles()).toBe(false);
});
it.each([
  ['missing remaining', { results: [{ id: '1', ok: true }] }],
  ['string remaining', { results: [{ id: '1', ok: true }], remaining: '0' }],
  ['negative remaining', { results: [{ id: '1', ok: true }], remaining: -1 }],
  ['missing results', { remaining: 0 }],
  ['results not an array', { results: {}, remaining: 0 }],
])('does not report success for an empty_trash_block with %s', async (_name, block) => {
  respond = command => Response.json(command === 'empty_trash_start' ? { token: 'snap', remaining: 1 } : block);
  const progress = vi.fn();
  await expect(emptyWebTrash({ count: 1, bytes: 0, maxDeletedAt: null }, 'secret', progress)).rejects.toMatchObject({ kind: 'not_started' });
  expect(calls).toHaveLength(2); expect(progress).not.toHaveBeenCalledWith(1, 1);
});
it('rejects an empty_trash_start without a usable token or remaining count', async () => {
  respond = () => Response.json({ remaining: 1 });
  await expect(emptyWebTrash({ count: 1, bytes: 0, maxDeletedAt: null }, 'secret', vi.fn())).rejects.toMatchObject({ kind: 'not_started' });
  expect(calls).toHaveLength(1);
});

// Befund 5: Meldungen und Auswahl
it('announces applied IDs for delete, restore and purge, and block results, but not for plain renames', async () => {
  const seen: string[][] = []; const off = subscribeFileOpsApplied(ids => seen.push(ids));
  respond = command => Response.json(command === 'delete_files' ? [{ id: '1', ok: true }, { id: '2', ok: true }] : command === 'delete_file_permanently' ? { kept_on_disk: false } : null);
  await fileOp('delete_files', { fileIds: ['1', '2'] }); await fileOp('restore_file', { fileId: '3' });
  await fileOp('delete_file_permanently', { fileId: '4', confirm: true, password: 'x' }); await fileOp('rename_file', { fileId: '5', name: 'n' });
  expect(seen).toEqual([['1', '2'], ['3'], ['4']]); off();
});
it('announces the deleted IDs of each trash block', async () => {
  const seen: string[][] = []; const off = subscribeFileOpsApplied(ids => seen.push(ids));
  respond = command => Response.json(command === 'empty_trash_start' ? { token: 's', remaining: 1 } : { results: [{ id: '9', ok: true }, { id: '10', ok: false, kind: 'conflict' }], remaining: 0 });
  await expect(emptyWebTrash({ count: 2, bytes: 0, maxDeletedAt: null }, 'p', vi.fn())).rejects.toBeDefined();
  expect(seen).toEqual([['9']]); off();
});
it('shows a dedicated message when only the reload fails after a successful action', async () => {
  const off = subscribeFileOpsRefresh(() => Promise.reject(new Error('reload')));
  await fileOp('restore_file', { fileId: '1' });
  expect(getMetadataError()).toBe('Aktion ausgeführt, Ansicht konnte nicht aktualisiert werden.'); off();
});
it('keeps the action error when the reload fails as well', async () => {
  const off = subscribeFileOpsRefresh(() => Promise.reject(new Error('reload')));
  respond = () => Response.json({ kind: 'conflict' }, { status: 409 });
  await expect(fileOp('restore_file', { fileId: '1' })).rejects.toBeDefined();
  expect(getMetadataError()).not.toContain('Ansicht'); expect(getMetadataError()).toContain('Ziel'); off();
});
it('reports an error and kept_on_disk exactly once', async () => {
  const listener = vi.fn(); const off = subscribeMetadataErrors(listener);
  respond = () => Response.json({ kept_on_disk: true });
  await fileOp('delete_file_permanently', { fileId: '1', confirm: true, password: 'x' }); expect(listener).toHaveBeenCalledTimes(1);
  listener.mockClear(); clearMetadataError(); listener.mockClear();
  await fileOp('delete_file_permanently', { fileId: '1', confirm: true, password: 'x' }, { quietKept: true }); expect(listener).not.toHaveBeenCalled();
  respond = () => Response.json([{ id: '1', ok: false, kind: 'cross_device' }, { id: '2', ok: false, kind: 'cross_device' }]);
  const ids = Array.from({ length: 201 }, (_, i) => String(i + 1)); clearMetadataError(); listener.mockClear();
  await expect(fileOpsBatch('delete_files', ids)).rejects.toBeDefined();
  expect(listener).toHaveBeenCalledTimes(2); // fileOp's error, then the batch's merged message superseding it; never a third
  off();
});
