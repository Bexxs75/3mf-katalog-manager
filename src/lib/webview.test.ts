import { expect, it, vi } from 'vitest';
import type { DragDropEvent } from '@tauri-apps/api/webview';
import type { Event } from '@tauri-apps/api/event';
const native = vi.hoisted(() => ({ handler: undefined as undefined | ((event: Event<DragDropEvent>) => void), active: false, unlisten: vi.fn() }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({
  onDragDropEvent: async (handler: (event: Event<DragDropEvent>) => void) => { native.handler = handler; return native.unlisten; },
}) }));
vi.mock('./nativeDrag', () => ({ NativeDragState: { isActive: () => native.active } }));
import { getCurrentWebview } from './webview';
it('suppresses every native drop event during drag-out and resumes afterward', async () => {
  const receive = vi.fn();
  const off = await getCurrentWebview().onDragDropEvent(receive);
  const events = ['enter', 'over', 'drop', 'leave'].map(type => ({ payload: { type, paths: ['/a.png'], position: { x: 0, y: 0 } } } as Event<DragDropEvent>));
  native.active = true;
  events.forEach(event => native.handler!(event));
  expect(receive).not.toHaveBeenCalled();
  native.active = false;
  events.forEach(event => native.handler!(event));
  expect(receive).toHaveBeenCalledTimes(4);
  off();
  expect(native.unlisten).toHaveBeenCalledOnce();
});
it('lets highlight observers see own-drag events while the default still suppresses them', async () => {
  native.active = true;
  const receive = vi.fn();
  await getCurrentWebview(true).onDragDropEvent(receive);
  native.handler!({ payload: { type: 'leave' } } as Event<DragDropEvent>);
  expect(receive).toHaveBeenCalledOnce();
  native.active = false;
});
