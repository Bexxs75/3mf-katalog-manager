import { useCallback, useState } from 'react';
import * as importExportApi from '../lib/api/importExport';

const CATALOG_SETTINGS_KEYS = [
  '3mf-katalog-theme',
  '3mf-katalog-display-preference',
  '3mf-katalog-language',
  '3mf-katalog-slicers',
  '3mf-katalog-density',
] as const;

/**
 * Schluessel, die zwar exportiert, beim Import aber NICHT wiederhergestellt
 * werden. `3mf-katalog-slicers` enthaelt Programmpfade, die spaeter per
 * `open_in_slicer` als Prozess gestartet werden - ein praepariertes Backup
 * koennte dort `/bin/sh` o.ae. hinterlegen (Security-Review 2026-09-19,
 * Finding Z-1). Slicer-Pfade sind ausserdem maschinenspezifisch und in einem
 * portablen Backup ohnehin wertlos; die automatische Erkennung
 * (`scanInstalledSlicers`) fuellt die Liste nach dem Import von selbst neu.
 */
const IMPORT_SKIPPED_SETTINGS_KEYS: ReadonlySet<string> = new Set(['3mf-katalog-slicers']);

/**
 * Erlaubte Werte je Einstellung - gespiegelt aus den Hooks/Contexts, die
 * diese Schluessel besitzen (useTheme, useDisplayPreference,
 * i18n/LanguageContext, UiDensityContext). Ein Wert aus einem fremden Backup,
 * der hier nicht auftaucht, wird stillschweigend uebersprungen (die
 * bestehende lokale Einstellung bleibt dann erhalten) - der Katalog-Import
 * selbst gilt weiterhin als erfolgreich.
 */
const IMPORT_ALLOWED_SETTINGS_VALUES: Record<string, readonly string[]> = {
  '3mf-katalog-theme': ['system', 'light', 'dark'],
  '3mf-katalog-display-preference': ['thumbnail', 'render'],
  '3mf-katalog-language': ['de', 'en', 'es', 'fr'],
  '3mf-katalog-density': ['compact', 'comfort'],
};

export function useCatalogBackup() {
  const [catalogBackupError, setCatalogBackupError] = useState<string | null>(null);

  const exportCatalog = useCallback(() => {
    setCatalogBackupError(null);
    const settings: Record<string, string | null> = {};
    for (const key of CATALOG_SETTINGS_KEYS) {
      settings[key] = localStorage.getItem(key);
    }
    return importExportApi.exportCatalog(JSON.stringify(settings)).catch((e) => {
      console.error('[catalog-backup] Export fehlgeschlagen:', e);
      setCatalogBackupError(String(e));
    });
  }, []);

  const importCatalog = useCallback((onImported: () => void) => {
    setCatalogBackupError(null);
    return importExportApi
      .importCatalog()
      .then((result) => {
        if (!result.imported) return;
        if (result.settingsJson) {
          // Ein Fehler beim Wiederherstellen der Einstellungen darf den
          // erfolgreichen Katalog-Import nicht als Fehlschlag erscheinen
          // lassen (Finding I2) - daher eigenes try/catch statt im
          // aeusseren .catch() der Promise-Kette landen zu lassen.
          try {
            const settings = JSON.parse(result.settingsJson) as Record<string, string | null>;
            for (const key of CATALOG_SETTINGS_KEYS) {
              if (IMPORT_SKIPPED_SETTINGS_KEYS.has(key)) continue;
              const value = settings[key];
              if (value === null || value === undefined) {
                localStorage.removeItem(key);
                continue;
              }
              const allowed = IMPORT_ALLOWED_SETTINGS_VALUES[key];
              if (allowed && !allowed.includes(value)) {
                console.warn(`[catalog-backup] Ungueltiger Wert fuer ${key} im Backup, wird ignoriert.`);
                continue;
              }
              localStorage.setItem(key, value);
            }
          } catch (e) {
            console.error('[catalog-backup] Einstellungen konnten nicht wiederhergestellt werden:', e);
          }
        }
        // Backend hat AppState.db bereits auf den neu importierten Katalog
        // umverbunden - der Aufrufer muss hier deshalb einen vollen Reload
        // ausloesen (Finding C1, siehe App.tsx), sonst zeigt das Frontend
        // weiter veraltete Modell-IDs aus dem alten Katalog an.
        onImported();
      })
      .catch((e) => {
        console.error('[catalog-backup] Import fehlgeschlagen:', e);
        setCatalogBackupError(String(e));
      });
  }, []);

  return { catalogBackupError, exportCatalog, importCatalog };
}
