import { invoke } from '@tauri-apps/api/core';

/** Two frames let the browser paint the mounted, themed surface before showing it. */
export function showAfterPaint(): () => void {
  let cancelled = false;
  let frame = 0;
  void document.fonts?.ready.then(() => paint());
  if (!document.fonts) paint();
  function paint() {
    if (cancelled) return;
    frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => {
        if (!cancelled && '__TAURI_INTERNALS__' in window) {
          void invoke('frontend_ready').catch(error => console.warn('[startup] showing window failed:', error));
        }
      });
    });
  }
  return () => { cancelled = true; cancelAnimationFrame(frame); };
}
