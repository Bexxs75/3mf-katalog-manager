import { useSyncExternalStore } from 'react';
import { listen } from './events';
import { getDragOutEnabled, startFileDrag } from './api/fileDrag';

let active = false;
export const NativeDragState = {
  isActive: () => active,
  finish: () => { active = false; },
};
export async function startNativeDrag(fileIds: string[]) {
  if (active) return;
  active = true;
  let off: (() => void) | undefined;
  try {
    off = await listen('native-drag-ended', () => { NativeDragState.finish(); off?.(); });
    await startFileDrag(fileIds);
    // On Linux/macOS invoke can resolve before the native session ends.
  } catch (error) {
    NativeDragState.finish(); off?.(); throw error;
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
