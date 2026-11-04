import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { useExternalDropTarget } from './useExternalDropTarget';
import { makeFolder } from '../test/factories';

const mocks = vi.hoisted(() => ({
  handler: null as null | ((event: { payload: unknown }) => void),
  unlisten: vi.fn(),
}));
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: (cb: (event: { payload: unknown }) => void) => { mocks.handler = cb; return Promise.resolve(mocks.unlisten); },
  }),
}));

const folders = [
  makeFolder({ id: 'r', name: 'Katalog', path: '/data/Katalog' }),
  makeFolder({ id: 's', name: 'Halter', path: '/data/Katalog/Halter', parentId: 'r' }),
  makeFolder({ id: 'g', name: 'Alt', path: '/data/Katalog/Alt', parentId: 'r', missing: true }),
];
function rowAt(id: string | null) {
  document.elementFromPoint = vi.fn(() => (id ? { closest: () => ({ getAttribute: () => id }) } : null)) as never;
}
const emit = (payload: unknown) => act(() => mocks.handler?.({ payload }));
const setup = (enabled = true) => renderHook(() => useExternalDropTarget({ enabled, folders, catalogBaseDir: '/data/Katalog' }));

beforeEach(() => { mocks.handler = null; });
afterEach(() => { vi.restoreAllMocks(); });

describe('useExternalDropTarget', () => {
  it('highlights the folder under the pointer only while dragging and counts the files', () => {
    const { result } = setup();
    expect(result.current.state.target.kind).toBe('none');
    rowAt('s');
    emit({ type: 'enter', paths: ['/a.3mf', '/b.3mf'], position: { x: 5, y: 5 } });
    expect(result.current.state.target).toMatchObject({ kind: 'folder', folder: { id: 's' } });
    expect(result.current.state.count).toBe(2);
    rowAt('g');
    emit({ type: 'over', position: { x: 5, y: 9 } });
    expect(result.current.state.target).toMatchObject({ kind: 'invalid', reason: 'missing' });
    expect(result.current.state.count).toBe(2);
    emit({ type: 'leave' });
    expect(result.current.state).toEqual({ target: { kind: 'none' }, count: 0 });
  });
  it('clears the highlight on drop', () => {
    const { result } = setup();
    rowAt('s');
    emit({ type: 'enter', paths: ['/a.3mf'], position: { x: 1, y: 1 } });
    emit({ type: 'drop', paths: ['/a.3mf'], position: { x: 1, y: 1 } });
    expect(result.current.state.target.kind).toBe('none');
  });
  it('does not listen when disabled', () => {
    setup(false);
    expect(mocks.handler).toBeNull();
  });
  it('resolve answers the drop position like the drag does', () => {
    const { result } = setup();
    rowAt('r');
    expect(result.current.resolve({ x: 1, y: 1 })).toMatchObject({ kind: 'invalid', reason: 'catalogRoot' });
    rowAt(null);
    expect(result.current.resolve({ x: 1, y: 1 }).kind).toBe('none');
  });
  it('keeps and dismisses the rejection of a drop', () => {
    const { result } = setup();
    const target = { kind: 'invalid', folder: folders[2], reason: 'missing' } as const;
    act(() => result.current.reject(target));
    expect(result.current.rejected).toBe(target);
    act(() => result.current.dismissRejected());
    expect(result.current.rejected).toBeNull();
  });
});
