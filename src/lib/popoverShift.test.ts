import { describe, expect, it } from 'vitest';
import { popoverShift } from './popoverShift';

describe('popoverShift', () => {
  it('keeps the visible box inside the margins for a scrolled document viewport rectangle', () => {
    const left = 20;
    const width = 950;
    const dx = popoverShift(left, width, 1000, 8);
    expect(dx).toBe(0);
    expect(left + dx).toBeGreaterThanOrEqual(8);
    expect(left + dx + width).toBeLessThanOrEqual(992);
  });

  it('leaves a popover that fits untouched', () => {
    expect(popoverShift(100, 250, 1280, 8)).toBe(0);
  });

  it('moves a popover sticking out on the right back by the overflow plus margin', () => {
    expect(popoverShift(863, 247, 1100, 8)).toBe(-18);
  });

  it('counts a popover ending exactly at the margin as fitting', () => {
    expect(popoverShift(842, 250, 1100, 8)).toBe(0);
  });

  it('keeps the left edge visible when the popover is wider than the window', () => {
    expect(popoverShift(50, 600, 400, 8)).toBe(-42);
  });

  it('pulls a popover starting left of the margin back in', () => {
    expect(popoverShift(-20, 100, 1000, 8)).toBe(28);
  });
});
