import { useEffect, useRef, useState } from 'react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { evaluateImageDrop, isOverDropZone, type ImageDropRejection } from '../lib/imageDrop';

interface Options {
  /** Nur lauschen, solange das Formular offen ist. */
  enabled: boolean;
  onImage: (path: string) => void;
  onReject: (reason: ImageDropRejection) => void;
}

/**
 * Datei-Drop auf ein Element (z. B. das Bildfeld im Spulenformular). Natives
 * HTML5-Drop kommt unter `dragDropEnabled` (fuer den Modell-Import in
 * useFileImport) im WebKitGTK-Fenster nie an: Deshalb wird Tauris
 * `onDragDropEvent` ausgewertet und die Position selbst gegen das Element
 * geprueft. Der Modell-Import ignoriert Drops ausserhalb des Katalogs
 * (useFileImport `enabled`), es gibt also keinen Doppel-Import.
 */
export function useImageDropZone<T extends HTMLElement>({ enabled, onImage, onReject }: Options) {
  const zoneRef = useRef<T>(null);
  const [over, setOver] = useState(false);
  const handlers = useRef({ onImage, onReject });
  useEffect(() => {
    handlers.current = { onImage, onReject };
  });

  useEffect(() => {
    if (!enabled) return;
    // Lokales Flag statt "mounted"-Ref: StrictMode (mount → cleanup → mount)
    // macht aus der ersten Anmeldung sauber eine tote, die zweite bleibt aktiv.
    let active = true;
    const zoneRect = () => zoneRef.current?.getBoundingClientRect() ?? null;
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      if (!active) return;
      const payload = event.payload;
      if (payload.type === 'enter' || payload.type === 'over') {
        setOver(isOverDropZone(payload.position, window.devicePixelRatio, zoneRect()));
        return;
      }
      setOver(false);
      if (payload.type !== 'drop') return;
      const result = evaluateImageDrop(payload.paths, payload.position, window.devicePixelRatio, zoneRect());
      if (result.kind === 'image') handlers.current.onImage(result.path);
      else if (result.kind === 'rejected') handlers.current.onReject(result.reason);
    });
    return () => {
      active = false;
      setOver(false);
      void unlisten.then((fn) => fn());
    };
  }, [enabled]);

  return { zoneRef, over: enabled && over };
}
