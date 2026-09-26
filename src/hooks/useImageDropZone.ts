import { useEffect, useRef, useState } from 'react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { evaluateImageDrop, isOverDropZone, isWindowsPlatform, type ImageDropRejection } from '../lib/imageDrop';

interface Options {
  /** Only listen while the form is open. */
  enabled: boolean;
  onImage: (path: string) => void;
  onReject: (reason: ImageDropRejection) => void;
}

/**
 * File drop onto an element (e.g. the image field in the spool form). Native
 * HTML5 drop never arrives in the WebKitGTK window under `dragDropEnabled`
 * (needed for model import in useFileImport): so Tauri's
 * `onDragDropEvent` is evaluated and the position is checked against the
 * element ourselves. The model import ignores drops outside the catalog
 * (useFileImport `enabled`), so there is no double import.
 */
export function useImageDropZone<T extends HTMLElement>({ enabled, onImage, onReject }: Options) {
  const zoneRef = useRef<T>(null);
  const [over, setOver] = useState(false);
  const handlers = useRef({ onImage, onReject });
  useEffect(() => {
    handlers.current = { onImage, onReject };
  });
  // Only on Windows does Tauri deliver physical pixels (see imageDrop.ts); the
  // platform doesn't change at runtime, so detect it once.
  const isWindows = useRef(isWindowsPlatform()).current;

  useEffect(() => {
    if (!enabled) return;
    // Local flag instead of a "mounted" ref: StrictMode (mount → cleanup → mount)
    // cleanly turns the first registration into a dead one, the second stays active.
    let active = true;
    const zoneRect = () => zoneRef.current?.getBoundingClientRect() ?? null;
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      if (!active) return;
      const payload = event.payload;
      if (payload.type === 'enter' || payload.type === 'over') {
        setOver(isOverDropZone(payload.position, window.devicePixelRatio, isWindows, zoneRect()));
        return;
      }
      setOver(false);
      if (payload.type !== 'drop') return;
      const result = evaluateImageDrop(payload.paths, payload.position, window.devicePixelRatio, isWindows, zoneRect());
      if (result.kind === 'image') handlers.current.onImage(result.path);
      else if (result.kind === 'rejected') handlers.current.onReject(result.reason);
    });
    return () => {
      active = false;
      setOver(false);
      // `listen()` may also reject only after the cleanup (especially in
      // StrictMode, double mount), or `fn()` may fail - without catch this
      // would be an unhandled rejection.
      unlisten
        .then((fn) => fn())
        .catch((e) => console.error('[useImageDropZone] unlisten failed:', e));
    };
  }, [enabled, isWindows]);

  return { zoneRef, over: enabled && over };
}
