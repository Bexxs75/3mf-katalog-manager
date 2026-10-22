import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useAutoTagBlock } from './useAutoTagBlock';
import * as api from '../lib/api/autoTags';

vi.mock('../lib/api/autoTags');

type Model = { id: string; name: string };
const modelA: Model = { id: 'a', name: 'A' };
const modelB: Model = { id: 'b', name: 'B' };

function deferred<T = void>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => { resolve = r; });
  return { promise, resolve };
}

// Callbacks are bound to the model they were created for, like the ones the workspace passes.
function setup() {
  const calls: string[] = [];
  const hook = renderHook(
    ({ model }: { model: Model }) => useAutoTagBlock(
      model,
      (t) => calls.push(`remove ${model.id} ${t}`),
      (t) => calls.push(`add ${model.id} ${t}`),
    ),
    { initialProps: { model: modelA } },
  );
  return { hook, calls };
}

beforeEach(() => {
  vi.mocked(api.listSuggestedAutoTags).mockReset().mockResolvedValue(['miniatur']);
  vi.mocked(api.blockAutoTag).mockReset().mockResolvedValue(undefined);
  vi.mocked(api.unblockAutoTag).mockReset().mockResolvedValue(undefined);
});

describe('useAutoTagBlock model switches', () => {
  it('shows the hint for the same model once the block is done', async () => {
    const { hook, calls } = setup();
    await act(async () => { await hook.result.current.removeAndBlock('miniatur'); });
    expect(hook.result.current.notice).toEqual({ tag: 'miniatur', undone: false });
    await act(async () => { await hook.result.current.undo(); });
    expect(calls).toEqual(['remove a miniatur', 'add a miniatur']);
  });

  it('shows no hint at B when the model changes while the block is still running', async () => {
    const gate = deferred();
    vi.mocked(api.blockAutoTag).mockReturnValue(gate.promise);
    const { hook, calls } = setup();
    let pending!: Promise<void>;
    act(() => { pending = hook.result.current.removeAndBlock('miniatur'); });
    hook.rerender({ model: modelB });
    await act(async () => { gate.resolve(); await pending; });
    expect(hook.result.current.notice).toBeNull();
    expect(calls).toEqual(['remove a miniatur']);
    await act(async () => { await hook.result.current.undo(); });
    expect(api.unblockAutoTag).not.toHaveBeenCalled();
    expect(calls.some((c) => c.startsWith('add'))).toBe(false);
  });

  it('drops the hint when switching to B after the action finished, and undo cannot reach B', async () => {
    const { hook, calls } = setup();
    await act(async () => { await hook.result.current.removeAndBlock('miniatur'); });
    hook.rerender({ model: modelB });
    expect(hook.result.current.notice).toBeNull();
    await act(async () => { await hook.result.current.undo(); });
    expect(api.unblockAutoTag).not.toHaveBeenCalled();
    expect(calls).toEqual(['remove a miniatur']);
  });

  it('adds the tag back to A when the model changes while the undo is running', async () => {
    const gate = deferred();
    const { hook, calls } = setup();
    await act(async () => { await hook.result.current.removeAndBlock('miniatur'); });
    vi.mocked(api.unblockAutoTag).mockReturnValue(gate.promise);
    let pending!: Promise<void>;
    act(() => { pending = hook.result.current.undo(); });
    hook.rerender({ model: modelB });
    await act(async () => { gate.resolve(); await pending; });
    expect(calls).toEqual(['remove a miniatur', 'add a miniatur']);
    expect(hook.result.current.notice).toBeNull();
  });
});
