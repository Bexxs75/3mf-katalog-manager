import { ModelViewer } from './ModelViewer';

interface Props {
  fileId: string;
  onSnapshotCaptured: (base64: string) => void;
  onError: () => void;
}

/**
 * Renders a snapshot for a single file invisibly in the background (real
 * size, but positioned outside the visible area and transparent).
 * ModelViewer needs a real container size (ResizeObserver-based) -
 * display:none would mean it never renders, hence opacity+position
 * instead of display.
 */
export function BackgroundSnapshotRenderer({ fileId, onSnapshotCaptured, onError }: Props) {
  return (
    <div
      style={{ position: 'fixed', top: -9999, left: -9999, width: 400, height: 300, opacity: 0, pointerEvents: 'none' }}
      aria-hidden="true"
    >
      <ModelViewer fileId={fileId} needsSnapshot onSnapshotCaptured={onSnapshotCaptured} onError={onError} />
    </div>
  );
}
