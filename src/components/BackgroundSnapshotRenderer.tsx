import { ModelViewer } from './ModelViewer';

interface Props {
  fileId: string;
  onSnapshotCaptured: (base64: string) => void;
}

/**
 * Rendert unsichtbar (echte Groesse, aber ausserhalb des sichtbaren
 * Bereichs positioniert und transparent) im Hintergrund einen
 * Schnappschuss fuer eine einzelne Datei nach. ModelViewer braucht eine
 * reale Container-Groesse (ResizeObserver-basiert) - display:none wuerde
 * dazu fuehren, dass nie gerendert wird, daher opacity+position statt
 * display.
 */
export function BackgroundSnapshotRenderer({ fileId, onSnapshotCaptured }: Props) {
  return (
    <div
      style={{ position: 'fixed', top: -9999, left: -9999, width: 400, height: 300, opacity: 0, pointerEvents: 'none' }}
      aria-hidden="true"
    >
      <ModelViewer fileId={fileId} needsSnapshot onSnapshotCaptured={onSnapshotCaptured} />
    </div>
  );
}
