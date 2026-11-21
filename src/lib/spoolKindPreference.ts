import { uiSettings } from './uiSettings';
import type { SpoolKind } from '../types';

const STORAGE_KEY = '3mf-katalog-filament-kind';

/** Last chosen kind in the inventory; without (readable) storage: filament. */
export function loadSpoolKind(): SpoolKind {
  try {
    return uiSettings.getItem(STORAGE_KEY) === 'resin' ? 'resin' : 'filament';
  } catch {
    return 'filament';
  }
}

export function saveSpoolKind(kind: SpoolKind): void {
  try {
    uiSettings.setItem(STORAGE_KEY, kind);
  } catch {
    // Storage blocked (e.g. private mode): the choice only applies to this session.
  }
}
