import { useEffect, useState } from 'react';
import { hasStepPreview } from '../lib/api/stepPreview';
import { isReadOnly } from '../lib/readOnly';
import { modelFileType, stlEntryState, type StlEntryState } from '../lib/stlExport';
import { useRuntimeEnvironment } from './useRuntimeEnvironment';

/** Whether "Save as STL" is shown for a model; the STEP check only runs for STEP files. */
export function useStlEntry(model: { fileType?: string; name?: string; path?: string }): StlEntryState {
  const { container } = useRuntimeEnvironment();
  const fileType = modelFileType(model);
  const isStep = fileType === 'stp' || fileType === 'step';
  const [stepPreview, setStepPreview] = useState<boolean | null>(null);
  useEffect(() => {
    if (!isStep || isReadOnly() || container) return;
    let cancelled = false;
    // Like the viewer: a failed check assumes support, so a hiccup does not hide the entry.
    hasStepPreview().catch(() => true).then(value => { if (!cancelled) setStepPreview(value); });
    return () => { cancelled = true; };
  }, [isStep, container]);
  return stlEntryState({ fileType, readOnly: isReadOnly(), container, hasStepPreview: stepPreview });
}
