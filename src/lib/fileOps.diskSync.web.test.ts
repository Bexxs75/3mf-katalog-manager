import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { authenticationFailed, canManageFiles, login } from './webSession';
import { applyDiskSync, diskSyncRetry, previewDiskSync, subscribeFileOpsApplied } from './fileOps';
import { fileOpsMessage } from './fileOpsErrors';

let calls: { command: string; args: Record<string, unknown> }[];
let respond: (command: string, args: Record<string, unknown>) => Response;
const blockResponse = (over: Record<string, unknown>) => Response.json({ seq: 0, results: [], done: 0, total: 25, remaining: 25, counts: {}, stopped: null, ...over });
beforeEach(async () => {
  calls = []; respond = () => Response.json(null); diskSyncRetry.delaysMs = [0, 0];
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    if (!url.includes('/invoke/')) return Response.json({ authenticated: true, writable: true, fileOps: true, csrfToken: 'token' });
    const command = url.split('/').pop()!; const args = JSON.parse(String(options?.body));
    calls.push({ command, args }); return respond(command, args);
  }));
  await login('password');
});
afterEach(() => { authenticationFailed(401); vi.unstubAllGlobals(); });

const blocks = () => calls.filter(c => c.command === 'disk_sync_apply_block');
const okRow = (key: string, kind = 'import') => ({ kind, key, ok: true, ...(kind === 'import' ? { fileId: `f${key}` } : {}) });

describe('previewDiskSync', () => {
  it('posts the three lists and returns the plan', async () => {
    respond = () => Response.json({ token: 't', expiresIn: 3600, counts: { import: 1, removeModels: 0, removeCompanions: 0 }, skipped: [], mostlyMissingRoots: [], removes: false });
    const plan = await previewDiskSync({ newFiles: [{ root: 0, relativePath: 'a.3mf' }], missingModels: [], danglingCompanions: [] });
    expect(plan.token).toBe('t');
    expect(calls[0]).toEqual({ command: 'disk_sync_preview', args: { newFiles: [{ root: 0, relativePath: 'a.3mf' }], missingModels: [], danglingCompanions: [] } });
  });
  it.each([
    [404, { kind: 'snapshot_not_found' }, 'snapshot_not_found'],
    [422, { kind: 'limit_exceeded' }, 'limit_exceeded'],
    [503, { message: 'maintenance' }, 'maintenance'],
    [503, { kind: 'snapshot_capacity' }, 'snapshot_capacity'],
  ])('maps %s %j to a readable refusal', async (status, body, kind) => {
    respond = () => Response.json(body, { status });
    await expect(previewDiskSync({ newFiles: [], missingModels: ['1'], danglingCompanions: [] })).rejects.toMatchObject({ kind, message: fileOpsMessage(kind), outcome: null });
  });
  it('keeps the Retry-After of a rate limit', async () => {
    respond = () => Response.json({ kind: 'rate_limited' }, { status: 429, headers: { 'Retry-After': '600' } });
    await expect(previewDiskSync({ newFiles: [], missingModels: ['1'], danglingCompanions: [] })).rejects.toMatchObject({ kind: 'rate_limited', retryAfter: 600 });
  });
  it('locks file actions after restart_required', async () => {
    respond = () => Response.json({ kind: 'restart_required' }, { status: 503 });
    await expect(previewDiskSync({ newFiles: [], missingModels: ['1'], danglingCompanions: [] })).rejects.toMatchObject({ kind: 'restart_required' });
    expect(canManageFiles()).toBe(false);
  });
});

describe('applyDiskSync', () => {
  it('starts with the confirmation, counts per kind and numbers the blocks from zero', async () => {
    let n = 0;
    respond = command => {
      if (command === 'disk_sync_apply_start') return Response.json({ total: 25, remaining: 25 });
      n += 1;
      return n === 1 ? blockResponse({ seq: 0, remaining: 15, results: [okRow('0:a.3mf'), okRow('7', 'removeModel'), { kind: 'import', key: '0:b.3mf', ok: false, reason: 'changed' }] })
        : n === 2 ? blockResponse({ seq: 1, remaining: 5, stopped: 'deadline', results: [okRow('9', 'removeCompanion')] })
        : blockResponse({ seq: 2, remaining: 0, results: [okRow('0:c.3mf')] });
    };
    const removed: string[][] = []; const off = subscribeFileOpsApplied(ids => removed.push(ids));
    const progress = vi.fn();
    const outcome = await applyDiskSync('tok', true, progress);
    off();
    expect(calls[0]).toEqual({ command: 'disk_sync_apply_start', args: { token: 'tok', confirmRemoval: true } });
    expect(blocks().map(c => c.args)).toEqual([{ token: 'tok', seq: 0 }, { token: 'tok', seq: 1 }, { token: 'tok', seq: 2 }]);
    expect(outcome).toMatchObject({ imported: 2, removedModels: 1, removedCompanions: 1, done: 25, total: 25, aborted: null, fileIds: ['f0:a.3mf', 'f0:c.3mf'], skipped: [{ kind: 'import', key: '0:b.3mf', reason: 'changed' }] });
    expect(progress.mock.calls.map(c => c[0])).toEqual([0, 10, 20, 25]);
    expect(removed).toEqual([['7']]);
  });

  it('asks again with the same seq when the answer was lost, then goes on', async () => {
    let lost = true;
    respond = command => {
      if (command === 'disk_sync_apply_start') return Response.json({ total: 2, remaining: 2 });
      if (lost) { lost = false; throw new TypeError('network'); }
      return blockResponse({ seq: 0, total: 2, remaining: 0, results: [okRow('0:a.3mf'), okRow('0:b.3mf')] });
    };
    const outcome = await applyDiskSync('tok', false, () => {});
    expect(blocks().map(c => c.args.seq)).toEqual([0, 0]);
    expect(outcome.imported).toBe(2);
  });

  it('gives up after two repeats and reports what was applied', async () => {
    let n = 0;
    respond = command => {
      if (command === 'disk_sync_apply_start') return Response.json({ total: 20, remaining: 20 });
      n += 1;
      if (n === 1) return blockResponse({ seq: 0, total: 20, remaining: 10, results: [okRow('0:a.3mf')] });
      throw new TypeError('network');
    };
    await expect(applyDiskSync('tok', false, () => {})).rejects.toMatchObject({ outcome: { imported: 1, done: 10, total: 20 } });
    expect(blocks().map(c => c.args.seq)).toEqual([0, 1, 1, 1]);
  });

  it('ends the run on maintenance and keeps the counters', async () => {
    let n = 0;
    respond = command => {
      if (command === 'disk_sync_apply_start') return Response.json({ total: 20, remaining: 20 });
      n += 1;
      return n === 1 ? blockResponse({ seq: 0, total: 20, remaining: 10, results: [okRow('0:a.3mf')] }) : Response.json({ message: 'maintenance' }, { status: 503 });
    };
    const outcome = await applyDiskSync('tok', false, () => {});
    expect(outcome).toMatchObject({ aborted: 'maintenance', imported: 1, done: 10, total: 20 });
    expect(blocks()).toHaveLength(2);
  });

  it('treats a stopped block (maintenance, session) as the end of the run', async () => {
    for (const stopped of ['maintenance', 'session'] as const) {
      calls = [];
      respond = command => command === 'disk_sync_apply_start' ? Response.json({ total: 20, remaining: 20 }) : blockResponse({ seq: 0, total: 20, remaining: 10, stopped, results: [okRow('0:a.3mf')] });
      const outcome = await applyDiskSync('tok', false, () => {});
      expect(outcome).toMatchObject({ aborted: stopped, imported: 1 });
      expect(blocks()).toHaveLength(1);
    }
  });

  it('stops between blocks when asked and sends no further block', async () => {
    let stop = false;
    respond = command => command === 'disk_sync_apply_start' ? Response.json({ total: 30, remaining: 30 }) : blockResponse({ seq: 0, total: 30, remaining: 20, results: [okRow('0:a.3mf')] });
    const outcome = await applyDiskSync('tok', false, done => { if (done > 0) stop = true; }, () => stop);
    expect(outcome).toMatchObject({ aborted: 'user', done: 10, total: 30 });
    expect(blocks()).toHaveLength(1);
  });

  it('refuses a start (limit, backup) without touching a block', async () => {
    respond = () => Response.json({ kind: 'backup_failed' }, { status: 503 });
    await expect(applyDiskSync('tok', true, () => {})).rejects.toMatchObject({ kind: 'backup_failed', outcome: null });
    expect(blocks()).toHaveLength(0);
  });

  it('does not loop on a block that did nothing', async () => {
    respond = command => command === 'disk_sync_apply_start' ? Response.json({ total: 5, remaining: 5 }) : blockResponse({ seq: 0, total: 5, remaining: 5 });
    await expect(applyDiskSync('tok', false, () => {})).rejects.toMatchObject({ kind: 'not_started' });
    expect(blocks()).toHaveLength(1);
  });

  it('does not repeat a block after a rate limit', async () => {
    respond = command => command === 'disk_sync_apply_start' ? Response.json({ total: 5, remaining: 5 }) : Response.json({ kind: 'rate_limited' }, { status: 429, headers: { 'Retry-After': '300' } });
    await expect(applyDiskSync('tok', false, () => {})).rejects.toMatchObject({ kind: 'rate_limited', retryAfter: 300 });
    expect(blocks()).toHaveLength(1);
  });
});
