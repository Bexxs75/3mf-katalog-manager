import { useEffect, useState } from 'react';
import { checkFilament } from '../lib/api/filamentCheck';
import type { FilamentCheck } from '../types';

interface FilamentCheckState {
  checks: Map<string, FilamentCheck> | null;
  error: boolean;
}

// Loads the filament check for the IDs (order matters). An answer for an
// ID list that has changed in the meantime is discarded. `refreshKey`
// additionally triggers a reload (e.g. after "Re-read metadata") without
// the ID list itself changing.
export function useFilamentCheck(fileIds: string[], refreshKey = ''): FilamentCheckState {
  const key = fileIds.join(',');
  const [state, setState] = useState<FilamentCheckState>({ checks: null, error: false });

  useEffect(() => {
    const ids = key === '' ? [] : key.split(',');
    if (ids.length === 0) {
      setState({ checks: new Map(), error: false });
      return;
    }
    let current = true;
    // Previous results stay visible while reloading (no flickering of the
    // queue icons on every reorder); only a previous error is reset.
    setState((prev) => ({ checks: prev.checks, error: false }));
    checkFilament(ids)
      .then((result) => {
        if (current) setState({ checks: new Map(result.map((c) => [c.fileId, c])), error: false });
      })
      .catch((e) => {
        console.error('[filament-check] Prüfung fehlgeschlagen:', e);
        if (current) setState({ checks: null, error: true });
      });
    return () => {
      current = false;
    };
    // refreshKey deliberately forces a reload without being used in the effect.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, refreshKey]);

  return state;
}
