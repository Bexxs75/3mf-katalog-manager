import { useCallback, useState } from 'react';
import * as slicerApi from '../lib/api/slicer';
import type { ModelFile, SlicerConfig } from '../types';

export function useSlicerLauncher(
  models: ModelFile[],
  slicers: SlicerConfig[],
  primaryId: string | null,
  onNeedsSetup: () => void,
) {
  const [slicerError, setSlicerError] = useState<string | null>(null);

  const openInSlicer = useCallback(
    (id: string) => {
      const model = models.find((m) => m.id === id);
      if (!model) return;
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
      slicerApi.openInSlicer(target.path, model.path).catch((e) => {
        console.error('[slicer] Start fehlgeschlagen:', e);
        setSlicerError(String(e));
      });
    },
    [models, slicers, primaryId, onNeedsSetup],
  );

  return { slicerError, openInSlicer };
}
