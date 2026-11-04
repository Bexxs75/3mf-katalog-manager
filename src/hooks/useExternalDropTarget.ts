import { createContext, useCallback, useContext, useEffect, useRef, useState } from 'react';
import { getCurrentWebview } from '../lib/webview';
import { isWindowsPlatform, type Point } from '../lib/imageDrop';
import { noDropTarget, resolveExternalDropTarget, type DropTarget } from '../lib/externalDrop';
import type { Folder } from '../types';

interface Options {
  /** Only listen while the catalog view takes drops (same condition as the import). */
  enabled: boolean;
  folders: readonly Folder[];
  catalogBaseDir: string | null;
}

/** What the folder tree shows while files are dragged in from the file manager. */
export interface ExternalDragState { target: DropTarget; count: number }
export const ExternalDragContext = createContext<ExternalDragState>({ target: noDropTarget, count: 0 });
export const useExternalDrag = () => useContext(ExternalDragContext);

const targetKey = (target: DropTarget) => (target.kind === 'none' ? '' : `${target.kind}:${target.folder.id}`);

/**
 * Which catalog folder an Explorer drag is over. Native drags send no mouse events to the
 * page under `dragDropEnabled`, so the position of Tauri's drag event is turned into a folder
 * row (`data-folder-id`) here. `resolve` is the same lookup for the drop itself
 * (`useFileImport`); the spool image zone (`useImageDropZone`) stays a separate listener.
 */
export function useExternalDropTarget({ enabled, folders, catalogBaseDir }: Options) {
  const [state, setState] = useState<ExternalDragState>({ target: noDropTarget, count: 0 });
  const [rejected, setRejected] = useState<Extract<DropTarget, { kind: 'invalid' }> | null>(null);
  const latest = useRef({ folders, catalogBaseDir });
  useEffect(() => { latest.current = { folders, catalogBaseDir }; });
  // Only Windows delivers physical pixels (see imageDrop.ts); the platform never changes.
  const isWindows = useRef(isWindowsPlatform()).current;
  const resolve = useCallback((position: Point): DropTarget =>
    resolveExternalDropTarget(position, window.devicePixelRatio, isWindows, latest.current.folders, latest.current.catalogBaseDir),
  [isWindows]);

  useEffect(() => {
    if (import.meta.env.VITE_TARGET === 'web' || !enabled) return;
    let active = true;
    const clear = () => setState((s) => (s.target.kind === 'none' && s.count === 0 ? s : { target: noDropTarget, count: 0 }));
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      if (!active) return;
      const payload = event.payload;
      if (payload.type === 'enter' || payload.type === 'over') {
        const target = resolve(payload.position);
        const count = payload.type === 'enter' ? payload.paths.length : undefined;
        setState((s) => (targetKey(s.target) === targetKey(target) && (count === undefined || s.count === count) ? s : { target, count: count ?? s.count }));
      } else clear();
    });
    return () => {
      active = false;
      clear();
      unlisten.then((fn) => fn()).catch((e) => console.error('[useExternalDropTarget] unlisten failed:', e));
    };
  }, [enabled, resolve]);

  return {
    state,
    resolve,
    rejected,
    reject: setRejected,
    dismissRejected: useCallback(() => setRejected(null), []),
  };
}
