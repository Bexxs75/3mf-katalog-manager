import { shouldIgnoreCatalogShortcut } from '../lib/keyboardGuard';
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
      if (!navigate || shouldIgnoreCatalogShortcut(event, { allowDetailViewToggleArrows: true }) || isEditing()) return;
      if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
      event.preventDefault();
      navigate(event.key === 'ArrowLeft' ? 'previous' : 'next');
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [navigate, isEditing]);
}
