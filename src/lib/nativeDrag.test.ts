import { beforeEach, expect, it, vi } from 'vitest';
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), off: vi.fn(), end: () => {} }));
vi.mock('./api/fileDrag', () => ({ startFileDrag: mocks.invoke, getDragOutEnabled: async () => false }));
vi.mock('./events', () => ({ listen: mocks.listen }));
import { NativeDragState, startNativeDrag } from './nativeDrag';
beforeEach(() => {
  NativeDragState.finish();
  mocks.invoke.mockReset().mockResolvedValue(undefined);
  mocks.listen.mockImplementation(async (_name, cb) => { mocks.end = cb; return mocks.off; });
});
it('keeps self drops suppressed until the native end event, even after invoke resolves', async () => {
  await startNativeDrag(['1','2']);
  expect(NativeDragState.isActive()).toBe(true);
  expect(mocks.invoke).toHaveBeenCalledWith(['1','2']);
  mocks.end();
  expect(NativeDragState.isActive()).toBe(false);
});
it('clears the guard and listener when native start fails', async () => {
  mocks.invoke.mockRejectedValue({message:'dragFailed'});
  await expect(startNativeDrag(['1'])).rejects.toEqual({message:'dragFailed'});
  expect(NativeDragState.isActive()).toBe(false);
  expect(mocks.off).toHaveBeenCalled();
});
it('refuses overlapping starts without releasing the active guard', async () => {
  await startNativeDrag(['1']);
  await startNativeDrag(['2']);
  expect(mocks.invoke).toHaveBeenCalledTimes(1);
  expect(NativeDragState.isActive()).toBe(true);
  mocks.end();
});
