const STORAGE_KEY = '3mf-katalog-detail-width';

function validWidth(px: number): boolean {
  return Number.isFinite(px) && px >= 300 && px <= 720;
}

export function loadDetailPanelWidth(): number | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    const px = raw === null || raw.trim() === '' ? NaN : Number(raw);
    return validWidth(px) ? px : null;
  } catch {
    return null;
  }
}

export function saveDetailPanelWidth(px: number | null): void {
  try {
    if (px === null || !validWidth(px)) localStorage.removeItem(STORAGE_KEY);
    else localStorage.setItem(STORAGE_KEY, String(px));
  } catch {
    // Storage blocked: the choice only applies to this session.
  }
}

export function clampDetailPanelWidth(px: number, workspaceWidth: number): number {
  // Below 620px the catalog reserve and panel minimum cannot both fit.
  const maximum = Math.max(300, Math.min(720, workspaceWidth * 0.6, workspaceWidth - 320));
  return Math.min(maximum, Math.max(300, Number.isFinite(px) ? px : 300));
}
