import { useSyncExternalStore } from 'react';
import type { ModelFile } from '../types';
import { listen } from './events';
import { getDragOutEnabled, startFileDrag } from './api/fileDrag';

let session: { finish: () => void } | undefined;
export const NativeDragState = {
  isActive: () => session !== undefined,
  finish: () => { session?.finish(); },
};
export async function startNativeDrag(fileIds: string[]) {
  if (session) return;
  let off: (() => void) | undefined;
  const current = { finish: () => {
    clearTimeout(timer);
    off?.();
    off = undefined;
    if (session === current) session = undefined;
  } };
  session = current;
  const timer = setTimeout(current.finish, 60_000);
  try {
    off = await listen('native-drag-ended', current.finish);
    if (session !== current) { off(); return; }
    await startFileDrag(fileIds);
    // On Linux/macOS invoke can resolve before the native session ends.
  } catch (error) {
    current.finish();
    throw error;
  }
}
let enabled = false;
let loading: Promise<void> | undefined;
const subscribers = new Set<() => void>();
export function setNativeDragEnabled(value: boolean) {
  enabled = value;
  subscribers.forEach(fn => fn());
}
export function loadNativeDragEnabled() {
  loading ??= getDragOutEnabled().then(value => setNativeDragEnabled(value === true)).catch(() => {});
  return loading;
}
const subscribe = (fn: () => void) => { subscribers.add(fn); return () => { subscribers.delete(fn); }; };
export const useNativeDragEnabled = () => useSyncExternalStore(subscribe, () => enabled);

export const MAX_NATIVE_DRAG_FILES = 64;
// Callers pass the resolved selection only when the grabbed card belongs to it.
export function nativeDragIdsFor(model: ModelFile, selection?: readonly ModelFile[]): string[] {
  const candidates = selection ?? [model];
  return candidates.filter(file => !file.deletedAt && /\.(3mf|stl|obj)$/i.test(file.path)).map(file => file.id);
}

export async function beginDragOut(ids: string[]) {
  if (!ids.length || ids.length > MAX_NATIVE_DRAG_FILES || NativeDragState.isActive()) return;
  await startNativeDrag(ids);
}
