import type { SpoolKind } from '../types';

const STORAGE_KEY = '3mf-katalog-filament-kind';

/** Zuletzt gewaehlte Art im Lager; ohne (lesbaren) Speicher: Filament. */
export function loadSpoolKind(): SpoolKind {
  try {
    return localStorage.getItem(STORAGE_KEY) === 'resin' ? 'resin' : 'filament';
  } catch {
    return 'filament';
  }
}

export function saveSpoolKind(kind: SpoolKind): void {
  try {
    localStorage.setItem(STORAGE_KEY, kind);
  } catch {
    // Speicher gesperrt (z. B. privater Modus): Auswahl gilt nur fuer diese Sitzung.
  }
}
