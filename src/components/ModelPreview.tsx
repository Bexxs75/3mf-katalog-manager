import type { ModelFile } from '../types';
import { ModelViewer } from './ModelViewer';
import type { ViewerActions } from './ViewerErrorCard';

interface Props extends ViewerActions {
  surfaceClassName?: string;
  model: ModelFile;
  needsSnapshot: boolean;
  onSnapshotCaptured: (base64: string) => void;
  onError?: () => void;
  showRotationControls?: boolean;
}

/**
 * The 3D viewer for a model. STEP files go through the same path as every other
 * format: a build without STEP support answers with a typed "unsupported" error,
 * which the viewer shows as its own explanation card.
 */
export function ModelPreview({ surfaceClassName, model, needsSnapshot, onSnapshotCaptured, onError, showRotationControls, onOpenInSlicer, onRemoveFromCatalog }: Props) {
  return (
    <ModelViewer
      surfaceClassName={surfaceClassName}
      model={model}
      onOpenInSlicer={onOpenInSlicer}
      onRemoveFromCatalog={onRemoveFromCatalog}
      fileId={model.id}
      needsSnapshot={needsSnapshot}
      onSnapshotCaptured={onSnapshotCaptured}
      onError={onError}
      showRotationControls={showRotationControls}
    />
  );
}
