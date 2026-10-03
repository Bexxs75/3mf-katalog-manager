import type { ModelFile } from '../types';
import { ModelViewer } from './ModelViewer';
import { StepPreviewHint } from './StepPreviewHint';
import { useHasStepPreview } from '../hooks/useHasStepPreview';
import { isStepFilePath } from '../lib/stepFile';

interface Props {
  model: ModelFile;
  needsSnapshot: boolean;
  onSnapshotCaptured: (base64: string) => void;
  onError?: () => void;
  showRotationControls?: boolean;
}

/**
 * Picks between the real 3D viewer and the STEP explainer. Builds without
 * STEP support can't render these files at all, so `ModelViewer` never even
 * mounts for them here - asking the backend for geometry it doesn't have
 * would only produce a guaranteed error.
 */
export function ModelPreview({ model, needsSnapshot, onSnapshotCaptured, onError, showRotationControls }: Props) {
  const hasStepPreview = useHasStepPreview();

  if (isStepFilePath(model.path)) {
    // Until the build's capabilities are known, mounting the viewer would
    // already fire the geometry request (and possibly flash its error).
    if (hasStepPreview === null) return null;
    if (hasStepPreview === false) return <StepPreviewHint />;
  }

  return (
    <ModelViewer
      fileId={model.id}
      needsSnapshot={needsSnapshot}
      onSnapshotCaptured={onSnapshotCaptured}
      onError={onError}
      showRotationControls={showRotationControls}
    />
  );
}
