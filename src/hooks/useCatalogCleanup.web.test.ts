import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { act, renderHook } from '@testing-library/react';
import { authenticationFailed, login } from '../lib/webSession';
import { useCatalogCleanup } from './useCatalogCleanup';
import { makeModelFile } from '../test/factories';

let calls: { command: string; args: Record<string, unknown> }[];
let respond: (command: string, args: Record<string, unknown>) => Response;
const ids = (n: number, from = 1) => Array.from({ length: n }, (_, i) => String(from + i));
beforeEach(async () => {
  calls = []; respond = () => Response.json(null);
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    if (!url.includes('/invoke/')) return Response.json({ authenticated: true, writable: true, fileOps: true, csrfToken: 'token' });
    const command = url.split('/').pop()!; const args = JSON.parse(String(options?.body));
    calls.push({ command, args }); return respond(command, args);
  }));
  await login('password');
});
afterEach(() => { authenticationFailed(401); vi.unstubAllGlobals(); });

const issues = (orphaned: string[]) => ({ orphaned: orphaned.map(id => makeModelFile({ id, name: `m${id}.3mf`, path: `/r/gone/m${id}.3mf` })), duplicateGroups: [] });
const scanned = async (orphaned: string[]) => {
  respond = () => Response.json(issues(orphaned));
  const hook = renderHook(() => useCatalogCleanup());
  await act(async () => { await hook.result.current.scanCatalogIssues(); });
  expect(calls[0].command).toBe('scan_catalog_issues');
  return hook;
};

describe('web cleanup removal', () => {
  it('sends 450 models in blocks of 200, 200 and 50 and closes when all are gone', async () => {
    const all = ids(450);
    const { result } = await scanned(all);
    respond = (_, args) => Response.json((args.fileIds as string[]).map(id => ({ id, ok: true })));
    const onDeleted = vi.fn();
    await act(async () => { await result.current.deleteSelectedCleanupFiles(all, onDeleted); });
    const deletes = calls.filter(c => c.command === 'delete_files').map(c => (c.args.fileIds as string[]).length);
    expect(deletes).toEqual([200, 200, 50]);
    expect(onDeleted).toHaveBeenCalledWith(all);
    expect(result.current.cleanupDialogOpen).toBe(false);
    expect(result.current.cleanupIssues).toBeNull();
    expect(result.current.cleanupError).toBeNull();
  });

  it('keeps failed and unstarted models in the dialog and reports only the removed ones', async () => {
    const all = ids(450);
    const { result } = await scanned(all);
    // 2 models of the second block fail; the third block is never started
    respond = (_, args) => {
      const block = args.fileIds as string[];
      return Response.json(block.map(id => ({ id, ok: !['250', '251'].includes(id), ...(['250', '251'].includes(id) ? { kind: 'protected' } : {}) })));
    };
    const onDeleted = vi.fn();
    await act(async () => { await result.current.deleteSelectedCleanupFiles(all, onDeleted); });
    expect(calls.filter(c => c.command === 'delete_files')).toHaveLength(2);
    const removed = onDeleted.mock.calls.flatMap(c => c[0] as string[]);
    expect(removed).toHaveLength(398);
    expect(removed).not.toContain('250');
    expect(removed).not.toContain('450');
    expect(result.current.cleanupDialogOpen).toBe(true);
    const left = result.current.cleanupIssues!.orphaned.map(m => m.id);
    expect(left).toHaveLength(52);
    expect(left).toEqual(expect.arrayContaining(['250', '251', '450']));
    expect(result.current.cleanupError).not.toBeNull();
  });

  it('keeps everything when the first block is refused', async () => {
    const { result } = await scanned(['1', '2']);
    respond = () => Response.json({ kind: 'rate_limited' }, { status: 429 });
    const onDeleted = vi.fn();
    await act(async () => { await result.current.deleteSelectedCleanupFiles(['1', '2'], onDeleted); });
    expect(onDeleted).not.toHaveBeenCalled();
    expect(result.current.cleanupIssues!.orphaned).toHaveLength(2);
    expect(result.current.cleanupDialogOpen).toBe(true);
    expect(result.current.cleanupError).not.toBeNull();
  });
});
