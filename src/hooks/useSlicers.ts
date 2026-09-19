import { useCallback, useEffect, useState } from 'react';
import * as catalogMetaApi from '../lib/api/catalogMeta';
import * as slicerApi from '../lib/api/slicer';
import type { SlicerDto } from '../lib/api/slicer';
import type { SlicerConfig } from '../types';

const PRIMARY_ID_STORAGE_KEY = '3mf-katalog-primary-slicer-id';
const HIDDEN_IDS_STORAGE_KEY = '3mf-katalog-hidden-slicer-ids';

function toSlicerConfig(dto: SlicerDto): SlicerConfig {
  return { id: dto.id, name: dto.name, path: dto.executablePath };
}

function loadHiddenIds(): Set<string> {
  try {
    const raw = localStorage.getItem(HIDDEN_IDS_STORAGE_KEY);
    const parsed = raw ? JSON.parse(raw) : [];
    return new Set(Array.isArray(parsed) ? parsed : []);
  } catch {
    return new Set();
  }
}

/**
 * Verwaltet die konfigurierten Slicer-Programme (Name + Pfad). Seit der
 * M-06-Haertung (Task 11) ist die `registered_slicers`-Tabelle im Backend
 * die alleinige Quelle der Wahrheit - dieser Hook haelt lediglich noch eine
 * rein lokale UI-Praeferenz (welcher Slicer ist "primaer") sowie eine rein
 * lokale "Ausgeblendet"-Liste (es gibt (noch) keinen Backend-Command zum
 * endgueltigen Entfernen eines registrierten Slicers - ein "Entfernen" in
 * der UI blendet den Eintrag daher nur lokal aus, loescht ihn aber nicht
 * aus der Registry; ein erneuter Scan/Neustart zeigt ihn nicht erneut an,
 * solange er ausgeblendet bleibt).
 */
export function useSlicers() {
  const [slicers, setSlicers] = useState<SlicerConfig[]>([]);
  const [primaryId, setPrimaryIdState] = useState<string | null>(
    () => localStorage.getItem(PRIMARY_ID_STORAGE_KEY),
  );
  const [hiddenIds, setHiddenIds] = useState<Set<string>>(loadHiddenIds);
  const [addSlicerError, setAddSlicerError] = useState<string | null>(null);

  const applyRegistry = useCallback((rows: SlicerDto[]) => {
    setSlicers(rows.map(toSlicerConfig));
  }, []);

  useEffect(() => {
    catalogMetaApi.scanInstalledSlicers()
      .then(applyRegistry)
      .catch((e) => {
        // Rein komfortsteigerndes Feature - ein Fehlschlag darf die App
        // nicht beeintraechtigen, nur geloggt werden.
        console.warn('[slicer-scan] Automatische Slicer-Erkennung fehlgeschlagen:', e);
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const addSlicer = useCallback(async () => {
    // M-06/P0: der native Datei-Dialog laeuft im Backend
    // (`pick_and_register_slicer`) - das Frontend uebergibt hier keinen
    // selbst konstruierten Pfad mehr.
    //
    // `pick_and_register_slicer` schlaegt in echten Szenarien durchaus
    // fehl (z.B. `executable_path` ist `UNIQUE` - ein bereits registrierter
    // Slicer erneut ausgewaehlt ergibt einen Datenbankfehler; oder
    // `validate_slicer_path` lehnt die getroffene Auswahl ab) - ohne
    // try/catch landete das bisher als unbehandelte Promise-Ablehnung,
    // ohne dass der Nutzer irgendeine Rueckmeldung bekam. Gleiches
    // Fehler-Anzeige-Muster wie `useCatalogBackup.ts`/`useCatalogCleanup.ts`
    // (eigener `*Error`-State, in der Settings-UI direkt angezeigt).
    try {
      setAddSlicerError(null);
      const picked = await slicerApi.pickAndRegisterSlicer();
      if (!picked) return null;
      const rows = await slicerApi.listRegisteredSlicers();
      applyRegistry(rows);
      setPrimaryIdState((prev) => {
        if (prev) return prev;
        localStorage.setItem(PRIMARY_ID_STORAGE_KEY, picked.id);
        return picked.id;
      });
      return picked;
    } catch (e) {
      console.error('[slicer-register] Registrierung fehlgeschlagen:', e);
      setAddSlicerError(String(e));
      return null;
    }
  }, [applyRegistry]);

  const removeSlicer = useCallback((id: string) => {
    setHiddenIds((prev) => {
      const next = new Set(prev);
      next.add(id);
      localStorage.setItem(HIDDEN_IDS_STORAGE_KEY, JSON.stringify([...next]));
      return next;
    });
    setPrimaryIdState((prev) => {
      if (prev !== id) return prev;
      localStorage.removeItem(PRIMARY_ID_STORAGE_KEY);
      return null;
    });
  }, []);

  const setPrimary = useCallback((id: string) => {
    localStorage.setItem(PRIMARY_ID_STORAGE_KEY, id);
    setPrimaryIdState(id);
  }, []);

  const visibleSlicers = slicers.filter((s) => !hiddenIds.has(s.id));

  return {
    slicers: visibleSlicers,
    primaryId,
    addSlicer,
    addSlicerError,
    removeSlicer,
    setPrimary,
  };
}
