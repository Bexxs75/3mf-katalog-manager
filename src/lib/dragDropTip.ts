export const DND_TIP_DISMISSED_KEY = '3mf-katalog-dnd-tip-dismissed';
const DISMISSED_EVENT = '3mf-katalog-dnd-tip-dismissed';

export function isDragDropTipDismissed() {
  try { return localStorage.getItem(DND_TIP_DISMISSED_KEY) !== null; }
  catch { return false; }
}

export function dismissDragDropTip() {
  // Storage may be unavailable; a completed move must still refresh the catalog.
  try { localStorage.setItem(DND_TIP_DISMISSED_KEY, 'true'); } catch { /* Session dismissal still works. */ }
  window.dispatchEvent(new Event(DISMISSED_EVENT));
}

export function subscribeDragDropTipDismissed(listener: () => void) {
  window.addEventListener(DISMISSED_EVENT, listener);
  return () => window.removeEventListener(DISMISSED_EVENT, listener);
}
