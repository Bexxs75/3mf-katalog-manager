import { useEffect } from 'react';

export type DetailDirection = 'previous' | 'next';

export function detailNeighbor(ids: string[], current: string | null, direction: DetailDirection): string | null {
  const index = current === null ? -1 : ids.indexOf(current);
  if (index < 0) return null;
  return ids[index + (direction === 'next' ? 1 : -1)] ?? null;
}

export function useDetailNavigation(
  navigate: ((direction: DetailDirection) => void) | undefined,
  isEditing: () => boolean,
) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!navigate || event.defaultPrevented || event.isComposing || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
      if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
      if (document.querySelector('[role="dialog"][aria-modal="true"], [data-navigation-menu], [role="menu"], .menu')) return;
      const blockedTarget = (target: EventTarget | null) => target instanceof Element &&
        !!target.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"]), [data-detail-viewer], [role="slider"], [role="separator"], [role="listbox"]');
      if (blockedTarget(event.target) || blockedTarget(document.activeElement) || isEditing()) return;
      event.preventDefault();
      navigate(event.key === 'ArrowLeft' ? 'previous' : 'next');
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [navigate, isEditing]);
}
