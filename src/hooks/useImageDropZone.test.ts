import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { useImageDropZone } from './useImageDropZone';

const mocks = vi.hoisted(() => ({
  handler: null as null | ((event: { payload: unknown }) => void),
  subscriptions: 0,
  unlisten: vi.fn(),
}));

vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: (cb: (event: { payload: unknown }) => void) => {
      mocks.handler = cb;
      mocks.subscriptions += 1;
      return Promise.resolve(mocks.unlisten);
    },
  }),
}));

function zone() {
  const el = document.createElement('button');
  el.getBoundingClientRect = () =>
    ({ left: 100, top: 100, right: 200, bottom: 200, width: 100, height: 100, x: 100, y: 100, toJSON: () => ({}) }) as DOMRect;
  return el;
}

function emit(payload: unknown) {
  act(() => mocks.handler?.({ payload }));
}

beforeEach(() => {
  mocks.handler = null;
  mocks.subscriptions = 0;
  mocks.unlisten.mockReset();
  Object.defineProperty(window, 'devicePixelRatio', { value: 2, configurable: true });
});
afterEach(() => {
  Object.defineProperty(window, 'devicePixelRatio', { value: 1, configurable: true });
});

function setup(enabled = true) {
  const onImage = vi.fn();
  const onReject = vi.fn();
  const hook = renderHook(({ on }) => useImageDropZone<HTMLButtonElement>({ enabled: on, onImage, onReject }), {
    initialProps: { on: enabled },
  });
  (hook.result.current.zoneRef as { current: HTMLButtonElement | null }).current = zone();
  return { ...hook, onImage, onReject };
}

describe('useImageDropZone', () => {
  it('highlights while a drag hovers over the zone (physical pixels / dpr)', () => {
    const { result } = setup();
    emit({ type: 'over', position: { x: 300, y: 300 } }); // 150/150 CSS: drin
    expect(result.current.over).toBe(true);
    emit({ type: 'over', position: { x: 100, y: 100 } }); // 50/50 CSS: draussen
    expect(result.current.over).toBe(false);
    emit({ type: 'enter', paths: ['/a.png'], position: { x: 300, y: 300 } });
    expect(result.current.over).toBe(true);
    emit({ type: 'leave' });
    expect(result.current.over).toBe(false);
  });

  it('hands over exactly one dropped image', () => {
    const { result, onImage, onReject } = setup();
    emit({ type: 'over', position: { x: 300, y: 300 } });
    emit({ type: 'drop', paths: ['/home/u/spule.png'], position: { x: 300, y: 300 } });
    expect(onImage).toHaveBeenCalledWith('/home/u/spule.png');
    expect(onReject).not.toHaveBeenCalled();
    expect(result.current.over).toBe(false);
  });

  it('reports several files or a non-image, and ignores drops elsewhere', () => {
    const { onImage, onReject } = setup();
    emit({ type: 'drop', paths: ['/a.png', '/b.png'], position: { x: 300, y: 300 } });
    emit({ type: 'drop', paths: ['/a.3mf'], position: { x: 300, y: 300 } });
    emit({ type: 'drop', paths: ['/a.png'], position: { x: 10, y: 10 } });
    expect(onReject.mock.calls).toEqual([['multiple'], ['not-image']]);
    expect(onImage).not.toHaveBeenCalled();
  });

  it('does not listen while disabled and unsubscribes on disable', async () => {
    const { rerender, result } = setup(false);
    expect(mocks.subscriptions).toBe(0);
    expect(result.current.over).toBe(false);
    rerender({ on: true });
    expect(mocks.subscriptions).toBe(1);
    rerender({ on: false });
    await Promise.resolve();
    expect(mocks.unlisten).toHaveBeenCalledTimes(1);
  });
});
