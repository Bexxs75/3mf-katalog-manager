import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { authenticationFailed, canManageFiles, login } from './webSession';
import { companionFileOp, emptyWebTrash, subscribeFileOpsApplied } from './fileOps';
import { lostAnswerRetry } from './transport';

let calls: { command: string; args: Record<string, unknown>; requestId: string | undefined }[];
let respond: (command: string, attempt: number) => Response | Promise<Response>;
let fileOps = true;
beforeEach(async () => {
  calls = []; respond = () => Response.json(null); fileOps = true;
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    if (!url.includes('/invoke/')) return Response.json({ authenticated: true, writable: true, fileOps, csrfToken: 'token' });
    const command = url.split('/').pop()!;
    calls.push({ command, args: JSON.parse(String(options?.body)), requestId: (options?.headers as Record<string, string>)['X-Request-Id'] });
    return respond(command, calls.length);
  }));
  await login('password');
});
afterEach(() => { authenticationFailed(401); vi.useRealTimers(); vi.unstubAllGlobals(); });

it.each(['remove_companion', 'restore_companion'] as const)('sends %s with the companion id and one request id', async command => {
  await companionFileOp(command, '12');
  expect(calls).toHaveLength(1);
  expect(calls[0].command).toBe(command);
  expect(calls[0].args).toEqual({ companionId: '12' });
  expect(calls[0].requestId).toMatch(/^[A-Za-z0-9_-]{16,64}$/);
});

it('repeats after a lost answer with the same request id, and the second answer counts', async () => {
  vi.useFakeTimers();
  const saved = lostAnswerRetry.delaysMs;
  respond = (_command, attempt) => attempt === 1 ? Response.json({ message: 'late', expected: true }, { status: 408 }) : Response.json(null);
  const pending = companionFileOp('remove_companion', '5');
  await vi.advanceTimersByTimeAsync(saved[0]);
  await pending;
  expect(calls).toHaveLength(2);
  expect(calls[1].requestId).toBe(calls[0].requestId);
  expect(calls[1].args).toEqual(calls[0].args);
});

it('says in words that a full model cannot take another file, without showing a path', async () => {
  respond = () => Response.json({ kind: 'companion_limit', message: 'companion_limit', expected: true }, { status: 422 });
  await expect(companionFileOp('restore_companion', '5')).rejects.toMatchObject({
    kind: 'companion_limit',
    message: expect.stringContaining('100'),
    plain: true,
  });
});

it('does nothing without the file-management right', async () => {
  fileOps = false;
  await login('password');
  expect(canManageFiles()).toBe(false);
  await expect(companionFileOp('remove_companion', '1')).rejects.toMatchObject({ kind: 'revoked' });
  expect(calls).toHaveLength(0);
});

it('blocks file management when the server asks for a restart', async () => {
  respond = () => Response.json({ kind: 'restart_required', message: 'restart_required', expected: false }, { status: 503 });
  await expect(companionFileOp('remove_companion', '1')).rejects.toMatchObject({ kind: 'restart_required' });
  expect(canManageFiles()).toBe(false);
});

it('confirms the number of removed companion files when emptying the trash and counts them in the progress', async () => {
  respond = command => command === 'empty_trash_start' ? Response.json({ token: 'snap', remaining: 3 })
    : Response.json({ results: [{ id: '4', ok: true }, { id: 'c9', ok: true }], remaining: 0 });
  const progress = vi.fn();
  await emptyWebTrash({ count: 2, bytes: 1, maxDeletedAt: '2026-10-01T00:00:00Z', looseCompanionCount: 1 }, 'secret', progress);
  expect(calls[0].command).toBe('empty_trash_start');
  expect(calls[0].args).toEqual({ expectedCount: 2, expectedCompanions: 1, maxDeletedAt: '2026-10-01T00:00:00Z', password: 'secret' });
  expect(progress).toHaveBeenNthCalledWith(1, 0, 3);
  expect(progress).toHaveBeenLastCalledWith(3, 3);
});

it('leaves the field out when there is nothing of that kind, and never announces a companion row id as a file id', async () => {
  respond = command => command === 'empty_trash_start' ? Response.json({ token: 'snap', remaining: 1 })
    : Response.json({ results: [{ id: 'c9', ok: true }], remaining: 0 });
  const applied = vi.fn();
  const off = subscribeFileOpsApplied(applied);
  await emptyWebTrash({ count: 0, bytes: 0, maxDeletedAt: null, looseCompanionCount: 1 }, 'secret', vi.fn());
  off();
  expect(applied).not.toHaveBeenCalled();
  await emptyWebTrash({ count: 1, bytes: 0, maxDeletedAt: null }, 'secret', vi.fn());
  const starts = calls.filter(call => call.command === 'empty_trash_start');
  expect(starts[1].args).not.toHaveProperty('expectedCompanions');
});
