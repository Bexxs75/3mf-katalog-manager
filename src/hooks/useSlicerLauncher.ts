import { useRuntimeEnvironment } from './useRuntimeEnvironment';
import { useCallback, useState } from 'react';
import * as slicerApi from '../lib/api/slicer';
import { toAppError, type AppError } from '../lib/errors';
import type { SlicerConfig } from '../types';

// `openInSlicer` only takes `modelId`/`slicerId`; path and validation live in the backend.
export function useSlicerLauncher(
  slicers: SlicerConfig[],
  primaryId: string | null,
  onNeedsSetup: () => void,
) {
  const { container } = useRuntimeEnvironment();
  const [slicerError, setSlicerError] = useState<AppError | null>(null);

  const openInSlicer = useCallback(
    (modelId: string) => {
      if (container) return;
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
        console.error('[slicer] launch failed:', e);
        setSlicerError(toAppError(e));
      });
    },
    [slicers, primaryId, onNeedsSetup, container],
  );

  return { slicerError, openInSlicer };
}
