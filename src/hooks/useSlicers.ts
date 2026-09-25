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
 * Slicer-Programme aus der Backend-Registry. Lokal bleiben nur der primaere
 * Slicer und eine "Ausgeblendet"-Liste: es gibt keinen Befehl zum Loeschen
 * aus der Registry, "Entfernen" blendet nur aus.
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
        // Komfortfunktion: ein Fehlschlag wird nur geloggt.
        console.warn('[slicer-scan] Automatische Slicer-Erkennung fehlgeschlagen:', e);
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const addSlicer = useCallback(async () => {
    // Der Datei-Dialog laeuft im Backend, das Frontend reicht keinen Pfad durch.
    // Scheitern ist realistisch (schon registriert = UNIQUE, oder
    // validate_slicer_path lehnt ab), deshalb die Anzeige per addSlicerError.
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
