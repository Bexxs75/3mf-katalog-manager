import { useEffect, useState } from 'react';
import { checkFilament } from '../lib/api/filamentCheck';
import type { FilamentCheck } from '../types';

interface FilamentCheckState {
  checks: Map<string, FilamentCheck> | null;
  error: boolean;
}

// Laedt die Filament-Pruefung fuer die IDs (Reihenfolge zaehlt). Eine Antwort
// fuer eine inzwischen geaenderte ID-Liste wird verworfen.
export function useFilamentCheck(fileIds: string[]): FilamentCheckState {
  const key = fileIds.join(',');
  const [state, setState] = useState<FilamentCheckState>({ checks: null, error: false });

  useEffect(() => {
    const ids = key === '' ? [] : key.split(',');
    if (ids.length === 0) {
      setState({ checks: new Map(), error: false });
      return;
    }
    let current = true;
    setState({ checks: null, error: false });
    checkFilament(ids)
      .then((result) => {
        if (current) setState({ checks: new Map(result.map((c) => [c.fileId, c])), error: false });
      })
      .catch((e) => {
        console.error('[filament-check] Pruefung fehlgeschlagen:', e);
        if (current) setState({ checks: null, error: true });
      });
    return () => {
      current = false;
    };
  }, [key]);

  return state;
}
