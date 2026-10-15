import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createRenderLoop } from './renderLoop';

const tick = (count = 1) => vi.advanceTimersByTime(count * 16);
beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal('requestAnimationFrame', vi.fn((cb: FrameRequestCallback) =>
    setTimeout(() => cb(performance.now()), 16)));
  vi.stubGlobal('cancelAnimationFrame', vi.fn((id: number) => clearTimeout(id)));
});
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); vi.unstubAllGlobals(); });

function setup() {
  const update = vi.fn(() => false);
  const render = vi.fn();
  const loop = createRenderLoop({ update, render });
  return { update, render, ...loop };
}

describe('render loop', () => {
  it('does not render during 100 idle ticks', () => {
    const { update, render } = setup();
    tick(100);
    expect(update).toHaveBeenCalledTimes(100);
    expect(render.mock.calls.length).toBeLessThanOrEqual(1);
  });
  it('renders every moving tick', () => {
    const { update, render } = setup();
    render.mockClear();
    update.mockReturnValue(true);
    tick(100);
    expect(render).toHaveBeenCalledTimes(100);
  });
  it('bridges damping gaps and renders 30 ticks after the last movement', () => {
    const { update, render } = setup();
    render.mockClear();
    update.mockReturnValueOnce(true).mockReturnValueOnce(true)
      .mockReturnValueOnce(false).mockReturnValueOnce(true);
    tick(4);
    expect(render).toHaveBeenCalledTimes(4);
    tick(30);
    expect(render).toHaveBeenCalledTimes(34);
    tick(100);
    expect(render).toHaveBeenCalledTimes(34);
  });
  it('coalesces wheel/change invalidations into one next frame plus 30 trailing frames', () => {
    const { invalidate, render } = setup();
    render.mockClear();
    invalidate(); invalidate();
    expect(render).not.toHaveBeenCalled();
    tick();
    expect(render).toHaveBeenCalledTimes(1);
    tick(100);
    expect(render).toHaveBeenCalledTimes(31);
  });
  it('restarts rendering after 100 idle ticks', () => {
    const { invalidate, render } = setup();
    tick(100);
    render.mockClear();
    invalidate(); tick(100);
    expect(render).toHaveBeenCalledTimes(31);
  });
  it('handles change events dispatched inside update in the same tick', () => {
    const { update, invalidate, render } = setup();
    render.mockClear();
    update.mockImplementationOnce(() => { invalidate(); return false; });
    tick(100);
    expect(render).toHaveBeenCalledTimes(31);
  });
  it('cancels the pending frame and ignores invalidation after stop', () => {
    const { stop, invalidate, update, render } = setup();
    render.mockClear();
    stop(); stop(); invalidate(); tick(100);
    expect(cancelAnimationFrame).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
    expect(update).not.toHaveBeenCalled();
    expect(render).not.toHaveBeenCalled();
  });
});
