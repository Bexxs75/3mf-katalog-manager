import { describe, expect, it } from 'vitest';
import { popoverFit } from './popoverFit';

describe('popoverFit', () => {
  it('keeps a list below when its capped natural height fits', () => {
    expect(popoverFit(100, 64, 500, 630, 8, 176, 378)).toEqual({ top: 100, maxHeight: 378 });
  });
  it('keeps a short list below even with less than the minimum space', () => {
    expect(popoverFit(520, 484, 40, 630)).toEqual({ top: 520, maxHeight: 102 });
  });
  it('opens upward with the same gap when more space is available above', () => {
    expect(popoverFit(520, 484, 500, 630, 8, 176, 378, 4)).toEqual({ top: 102, maxHeight: 378 });
  });
  it('stays below when both sides are cramped', () => {
    expect(popoverFit(90, 54, 500, 180)).toEqual({ top: 90, maxHeight: 82 });
  });
  it('limits the height below when there is at least the minimum space', () => {
    expect(popoverFit(400, 364, 500, 630)).toEqual({ top: 400, maxHeight: 222 });
  });
  it('never returns negative heights, even for a zero-height viewport', () => {
    expect(popoverFit(520, 484, 500, 0)).toEqual({ top: 0, maxHeight: 0 });
  });
  it('clamps an offscreen menu into a very small viewport', () => {
    const result = popoverFit(50, 14, 500, 20);
    expect(result.maxHeight).toBeGreaterThanOrEqual(0);
    expect(result.top).toBeGreaterThanOrEqual(8);
    expect(result.top + result.maxHeight).toBeLessThanOrEqual(12);
  });
});
