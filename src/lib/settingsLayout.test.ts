import { describe, expect, it } from 'vitest';
import {
  resolveSettingsLayout, SETTINGS_MIN_WORKSPACE_WIDTH, SETTINGS_RAIL_WIDTH,
} from './settingsLayout';

describe('resolveSettingsLayout', () => {
  it.each([
    // window width, overlay, panel width (null = full), padding
    [320, true, null, 16],
    [599, true, null, 16],
    [600, true, 480, 20],
    [800, true, 480, 20],
    [999, true, 480, 20],
    [1000, true, 440, 20],
    [1279, false, 440, 20],
    [1280, false, 480, 20],
    [1920, false, 480, 20],
  ])('window %i px -> overlay %s, width %s, padding %i', (width, overlay, panel, pad) => {
    expect(resolveSettingsLayout(width)).toEqual({ overlay, width: panel, pad });
  });

  it('keeps the overlay within the available width minus 32 px', () => {
    for (const width of [600, 700, 999, 1000, 1100, 1500]) {
      const layout = resolveSettingsLayout(width, true);
      expect(layout.width).toBeLessThanOrEqual(width - SETTINGS_RAIL_WIDTH - 32);
    }
  });

  it('docks only while the catalog keeps at least 520 px', () => {
    // 440 px panel: the workspace keeps width - rail - 440.
    const edge = SETTINGS_MIN_WORKSPACE_WIDTH + SETTINGS_RAIL_WIDTH + 440;
    expect(resolveSettingsLayout(edge - 1).overlay).toBe(true);
    expect(resolveSettingsLayout(edge).overlay).toBe(false);
    expect(resolveSettingsLayout(edge)).toMatchObject({ width: 440 });
    // 480 px panel at 1280: 1280 - 61 - 480 = 739, well above the limit.
    expect(resolveSettingsLayout(1280).overlay).toBe(false);
  });

  it('is an overlay whenever the view cannot dock it, with the same width rules', () => {
    expect(resolveSettingsLayout(1600, true)).toEqual({ overlay: true, width: 480, pad: 20 });
    expect(resolveSettingsLayout(1100, true)).toEqual({ overlay: true, width: 440, pad: 20 });
    expect(resolveSettingsLayout(500, true)).toEqual({ overlay: true, width: null, pad: 16 });
  });
});
