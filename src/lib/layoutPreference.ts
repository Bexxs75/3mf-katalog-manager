export type LayoutMode = 'dashboard' | 'list';

const STORAGE_KEY = '3mf-katalog-filament-layout';

export function loadLayout(): LayoutMode {
  try {
    return localStorage.getItem(STORAGE_KEY) === 'list' ? 'list' : 'dashboard';
  } catch {
    return 'dashboard';
  }
}

export function saveLayout(layout: LayoutMode): void {
  try {
    localStorage.setItem(STORAGE_KEY, layout);
  } catch {
    // Storage blocked: the choice only applies to this session.
  }
}
