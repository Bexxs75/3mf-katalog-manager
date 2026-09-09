import { useCallback, useState } from 'react';
import type { SlicerConfig } from '../types';

const STORAGE_KEY = '3mf-katalog-slicers';

interface StoredState {
  slicers: SlicerConfig[];
  lastUsedId: string | null;
}

function loadStored(): StoredState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { slicers: [], lastUsedId: null };
    const parsed = JSON.parse(raw) as StoredState;
    return {
      slicers: Array.isArray(parsed.slicers) ? parsed.slicers : [],
      lastUsedId: typeof parsed.lastUsedId === 'string' ? parsed.lastUsedId : null,
    };
  } catch {
    return { slicers: [], lastUsedId: null };
  }
}

/**
 * Verwaltet die vom Nutzer konfigurierten Slicer-Programme (Name + Pfad).
 * Persistiert in localStorage nach demselben Muster wie Theme/Sprache
 * (useTheme.ts) - keine Backend-/DB-Beteiligung noetig fuer eine kurze
 * Konfigurationsliste.
 */
export function useSlicers() {
  const [state, setState] = useState<StoredState>(loadStored);

  const addSlicer = useCallback((name: string, path: string) => {
    setState((prev) => {
      const next: StoredState = {
        ...prev,
        slicers: [...prev.slicers, { id: crypto.randomUUID(), name, path }],
      };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  const removeSlicer = useCallback((id: string) => {
    setState((prev) => {
      const next: StoredState = {
        slicers: prev.slicers.filter((s) => s.id !== id),
        lastUsedId: prev.lastUsedId === id ? null : prev.lastUsedId,
      };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  const setLastUsed = useCallback((id: string) => {
    setState((prev) => {
      const next: StoredState = { ...prev, lastUsedId: id };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  return {
    slicers: state.slicers,
    lastUsedId: state.lastUsedId,
    addSlicer,
    removeSlicer,
    setLastUsed,
  };
}
