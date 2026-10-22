export type GroupMode = 'grouped' | 'single';

const STORAGE_KEY = '3mf-katalog-filament-group-mode';

/** Last chosen view of the inventory; without (readable) storage: grouped. */
export function loadGroupMode(): GroupMode {
  try {
    return localStorage.getItem(STORAGE_KEY) === 'single' ? 'single' : 'grouped';
  } catch {
    return 'grouped';
  }
}

export function saveGroupMode(mode: GroupMode): void {
  try {
    localStorage.setItem(STORAGE_KEY, mode);
  } catch {
    // Storage blocked: the choice only applies to this session.
  }
}
