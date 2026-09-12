import { useCallback, useState } from 'react';
import type { SlicerConfig } from '../types';

const STORAGE_KEY = '3mf-katalog-slicers';

interface DetectedSlicerInput {
  name: string;
  path: string;
}

interface StoredState {
  slicers: SlicerConfig[];
  lastUsedId: string | null;
  dismissedPaths: string[];
}

function loadStored(): StoredState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { slicers: [], lastUsedId: null, dismissedPaths: [] };
    const parsed = JSON.parse(raw) as Partial<StoredState>;
    const slicers = Array.isArray(parsed.slicers) ? parsed.slicers : [];
    return {
      // Aeltere, vor diesem Feature persistierte Eintraege haben noch kein
      // source-Feld - werden als 'manual' behandelt, da sie ausschliesslich
      // ueber den Datei-Dialog entstanden sein koennen.
      slicers: slicers.map((s) => ({ ...s, source: s.source ?? 'manual' })),
      lastUsedId: typeof parsed.lastUsedId === 'string' ? parsed.lastUsedId : null,
      dismissedPaths: Array.isArray(parsed.dismissedPaths) ? parsed.dismissedPaths : [],
    };
  } catch {
    return { slicers: [], lastUsedId: null, dismissedPaths: [] };
  }
}

/**
 * Verwaltet die konfigurierten Slicer-Programme (Name + Pfad), sowohl
 * manuell hinzugefuegte als auch automatisch erkannte. Persistiert in
 * localStorage nach demselben Muster wie Theme/Sprache (useTheme.ts) -
 * keine Backend-/DB-Beteiligung noetig fuer eine kurze Konfigurationsliste.
 */
export function useSlicers() {
  const [state, setState] = useState<StoredState>(loadStored);

  const addSlicer = useCallback((name: string, path: string) => {
    setState((prev) => {
      const next: StoredState = {
        ...prev,
        slicers: [...prev.slicers, { id: crypto.randomUUID(), name, path, source: 'manual' }],
      };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      return next;
    });
  }, []);

  const removeSlicer = useCallback((id: string) => {
    setState((prev) => {
      const removed = prev.slicers.find((s) => s.id === id);
      const next: StoredState = {
        slicers: prev.slicers.filter((s) => s.id !== id),
        lastUsedId: prev.lastUsedId === id ? null : prev.lastUsedId,
        dismissedPaths:
          removed && removed.source === 'auto'
            ? [...prev.dismissedPaths, removed.path]
            : prev.dismissedPaths,
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

  const mergeDetected = useCallback((detected: DetectedSlicerInput[]) => {
    setState((prev) => {
      const knownPaths = new Set(prev.slicers.map((s) => s.path));
      const dismissed = new Set(prev.dismissedPaths);
      const additions: SlicerConfig[] = [];
      for (const d of detected) {
        if (knownPaths.has(d.path) || dismissed.has(d.path)) continue;
        // Verhindert doppelte Eintraege, falls `detected` selbst denselben
        // Pfad mehrfach enthaelt (z.B. zwei Scan-Treffer fuer denselben Slicer).
        knownPaths.add(d.path);
        additions.push({ id: crypto.randomUUID(), name: d.name, path: d.path, source: 'auto' });
      }
      if (additions.length === 0) return prev;
      const next: StoredState = { ...prev, slicers: [...prev.slicers, ...additions] };
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
    mergeDetected,
  };
}
