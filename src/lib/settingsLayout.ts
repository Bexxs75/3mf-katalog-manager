/**
 * Width rules of the settings panel, in one place. The panel and its tests both read them from here;
 * the CSS only consumes the resulting variables (`--settings-panel-width`, `--settings-pad`).
 */

/** Width of the navigation rail (60 px plus its 1 px border) that sits left of the main area. */
export const SETTINGS_RAIL_WIDTH = 61;
/** Workspace remaining at the exact 1000 px docking threshold (1000 - 61 - 440). */
export const SETTINGS_MIN_WORKSPACE_WIDTH = 499;
/** From this window width on the panel may dock at all; below it the panel is always an overlay. */
export const SETTINGS_DOCK_MIN_WINDOW = 1000;
/** From this window width on the docked panel is wide. */
export const SETTINGS_WIDE_WINDOW = 1280;
export const SETTINGS_WIDTH_WIDE = 480;
export const SETTINGS_WIDTH_MEDIUM = 440;
/** Below this window width the overlay covers the whole available width and the padding shrinks. */
export const SETTINGS_FULL_WIDTH_BELOW = 600;
/** Space an overlay leaves free at the edge when it does not cover the full width. */
export const SETTINGS_OVERLAY_MARGIN = 32;
export const SETTINGS_PAD = 20;
export const SETTINGS_PAD_COMPACT = 16;

export interface SettingsLayout {
  /** True when the panel lies over the main area instead of taking space beside it. */
  overlay: boolean;
  /** Panel width in px, or null for the full available width. */
  width: number | null;
  /** Inner padding in px. */
  pad: number;
}

/**
 * @param windowWidth CSS width of the window.
 * @param forceOverlay The view cannot dock the panel (detail page, other main views, narrow catalog).
 */
export function resolveSettingsLayout(windowWidth: number, forceOverlay = false): SettingsLayout {
  const pad = windowWidth < SETTINGS_FULL_WIDTH_BELOW ? SETTINGS_PAD_COMPACT : SETTINGS_PAD;
  const panelWidth = windowWidth >= SETTINGS_WIDE_WINDOW ? SETTINGS_WIDTH_WIDE
    : windowWidth >= SETTINGS_DOCK_MIN_WINDOW ? SETTINGS_WIDTH_MEDIUM : SETTINGS_WIDTH_WIDE;
  const available = Math.max(0, windowWidth - SETTINGS_RAIL_WIDTH);
  const docked = !forceOverlay && windowWidth >= SETTINGS_DOCK_MIN_WINDOW
    && available - panelWidth >= SETTINGS_MIN_WORKSPACE_WIDTH;
  if (docked) return { overlay: false, width: panelWidth, pad };
  if (windowWidth < SETTINGS_FULL_WIDTH_BELOW) return { overlay: true, width: null, pad };
  return { overlay: true, width: Math.min(SETTINGS_WIDTH_WIDE, Math.max(0, available - SETTINGS_OVERLAY_MARGIN)), pad };
}
