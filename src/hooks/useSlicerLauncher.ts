import { useCallback, useState } from 'react';
import * as slicerApi from '../lib/api/slicer';
import type { SlicerConfig } from '../types';

// M-06 (Task 11): `openInSlicer` nimmt seit der Backend-Haertung keinen
// freien Pfad mehr entgegen, sondern ausschliesslich `modelId`/`slicerId` -
// die Pfad-Aufloesung/-Validierung passiert serverseitig gegen die
// `registered_slicers`-Registry. `models` (vollstaendige `ModelFile[]`)
// wird deshalb hier nicht mehr gebraucht.
export function useSlicerLauncher(
  slicers: SlicerConfig[],
  primaryId: string | null,
  onNeedsSetup: () => void,
) {
  const [slicerError, setSlicerError] = useState<string | null>(null);

  const openInSlicer = useCallback(
    (modelId: string) => {
      if (slicers.length === 0) {
        onNeedsSetup();
        return;
      }
      const target = slicers.find((s) => s.id === primaryId) ?? slicers[0];
      if (!target) {
        onNeedsSetup();
        return;
      }
      setSlicerError(null);
      slicerApi.openInSlicer(modelId, target.id).catch((e) => {
        console.error('[slicer] Start fehlgeschlagen:', e);
        setSlicerError(String(e));
      });
    },
    [slicers, primaryId, onNeedsSetup],
  );

  return { slicerError, openInSlicer };
}
