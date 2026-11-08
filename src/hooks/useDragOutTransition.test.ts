import { act, renderHook } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { makeModelFile } from '../test/factories';
const mocks = vi.hoisted(() => ({ enabled: true, readOnly: false, environment: { web: false, container: false }, begin: vi.fn(), active: false, info: vi.fn().mockResolvedValue(undefined) }));
vi.mock('../lib/nativeDrag', async importOriginal => ({ ...await importOriginal<typeof import('../lib/nativeDrag')>(), useNativeDragEnabled: () => mocks.enabled, loadNativeDragEnabled: async () => {}, beginDragOut: mocks.begin, NativeDragState: { isActive: () => mocks.active } }));
vi.mock('../lib/readOnly', () => ({ isReadOnly: () => mocks.readOnly }));
vi.mock('./useRuntimeEnvironment', () => ({ useRuntimeEnvironment: () => mocks.environment }));
vi.mock('../lib/logger', () => ({ info: mocks.info }));
vi.mock('../lib/events', () => ({ listen: async () => () => {} }));
import { useDragOutTransition } from './useDragOutTransition';
beforeEach(() => { mocks.enabled = true; mocks.readOnly = false; mocks.environment = { web: false, container: false }; mocks.active = false; mocks.begin.mockReset().mockResolvedValue(undefined); mocks.info.mockClear(); });
const model = makeModelFile({ id: 'a', path: '/a.3mf' });
function setup(models = [model]) {
  const handover = vi.fn();
  return { handover, ...renderHook(({ active }) => useDragOutTransition({ active, ids: models.map(m => m.id), models, onHandOver: handover }), { initialProps: { active: true } }) };
}
const move = (x: number, y: number, buttons = 1) => act(() => { window.dispatchEvent(new MouseEvent('mousemove', { clientX: x, clientY: y, buttons })); });
it.each([[-1, 10], [10, -1], [window.innerWidth, 10], [10, window.innerHeight]])('hands over beyond edge %s,%s once per drag', (x, y) => {
  const { handover, rerender } = setup();
  move(x, y); move(x, y);
  expect(handover).toHaveBeenCalledTimes(1);
  expect(mocks.begin).toHaveBeenCalledWith(['a']);
  rerender({ active: false }); rerender({ active: true }); move(x, y);
  expect(handover).toHaveBeenCalledTimes(2);
});
it('ignores inside coordinates and released buttons; mouseleave also triggers', () => {
  const { handover } = setup(); move(0, 0); move(-1, 0, 0);
  expect(handover).not.toHaveBeenCalled();
  act(() => document.documentElement.dispatchEvent(new MouseEvent('mouseleave', { buttons: 1 })));
  expect(handover).toHaveBeenCalledTimes(1);
});
it.each(['web', 'container', 'readOnly'] as const)('does nothing in %s', kind => {
  if (kind === 'readOnly') mocks.readOnly = true; else mocks.environment[kind] = true;
  const { result } = setup(); move(-1, 0);
  expect(mocks.begin).not.toHaveBeenCalled(); expect(result.current.hint).toBeNull();
});
it('shows disabled hint only once across mounts in a session and never cancels the local drag', () => {
  mocks.enabled = false;
  const first = setup(); move(-1, 0);
  expect(first.result.current.hint).toBe('dgOutsideOffHint');
  expect(first.handover).not.toHaveBeenCalled(); first.unmount();
  const second = setup(); move(-1, 0);
  expect(second.result.current.hint).toBeNull(); expect(mocks.begin).not.toHaveBeenCalled();
});
it('refuses more than 64 eligible files', () => {
  const { result, handover } = setup(Array.from({ length: 65 }, (_, i) => makeModelFile({ id: String(i), path: '/a.stl' })));
  move(-1, 0); expect(result.current.hint).toBe('dgLimit'); expect(handover).not.toHaveBeenCalled();
});
it('shows STEP hint and ignores deleted files', () => {
  const first = setup([makeModelFile({ path: '/a.STEP' })]); move(-1, 0);
  expect(first.result.current.hint).toBe('dgStepHint'); first.unmount();
  const second = setup([makeModelFile({ path: '/a.3mf', deletedAt: 'today' })]); move(-1, 0);
  expect(second.result.current.hint).toBeNull(); expect(mocks.begin).not.toHaveBeenCalled();
});
it('silently refuses while a native session is still active', () => {
  mocks.active = true; const { handover } = setup(); move(-1, 0);
  expect(handover).not.toHaveBeenCalled(); expect(mocks.begin).not.toHaveBeenCalled();
});
it('logs only one concise line per handover, without observing subsequent events', () => {
  const { rerender, unmount } = setup(); move(-1, 0); rerender({ active: false }); move(-2, 0);
  act(() => window.dispatchEvent(new MouseEvent('mouseup')));
  act(() => window.dispatchEvent(new MouseEvent('mousedown')));
  unmount();
  expect(mocks.info).toHaveBeenCalledExactlyOnceWith('dragout handover trigger=mousemove files=1');
});
it('logs mouseleave and the eligible file count', () => {
  setup([model, makeModelFile({ id: 'b', path: '/b.obj' }), makeModelFile({ id: 'step', path: '/c.step' })]);
  act(() => document.documentElement.dispatchEvent(new MouseEvent('mouseleave', { buttons: 1 })));
  expect(mocks.info).toHaveBeenCalledExactlyOnceWith('dragout handover trigger=mouseleave files=2');
});
