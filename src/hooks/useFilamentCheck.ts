import { useEffect, useState } from 'react';
import { checkFilament } from '../lib/api/filamentCheck';
import type { FilamentCheck } from '../types';

interface FilamentCheckState {
  checks: Map<string, FilamentCheck> | null;
  error: boolean;
}

// Laedt die Filament-Pruefung fuer die IDs (Reihenfolge zaehlt). Eine Antwort
// fuer eine inzwischen geaenderte ID-Liste wird verworfen. `refreshKey` loest
// zusaetzlich ein Neuladen aus (z. B. nach "Metadaten neu einlesen"), ohne
// dass sich die ID-Liste selbst aendert.
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
    // Bisherige Ergebnisse bleiben waehrend des Nachladens sichtbar (kein
    // Flackern der Warteschlangen-Symbole bei jeder Umsortierung); nur ein
    // vorheriger Fehler wird zurueckgesetzt.
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
    // refreshKey erzwingt bewusst ein Neuladen ohne eigene Verwendung im Effekt.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, refreshKey]);

  return state;
}
