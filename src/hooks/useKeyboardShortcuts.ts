import { useEffect } from 'react';

export const SEARCH_INPUT_ID = 'catalog-search-input';
// Set by ModelGrid/ModelList on every model tile/row (see there) -
// the only coupling point between grid markup and this purely
// position-based navigation.
export const MODEL_TILE_ATTR = 'data-model-id';

interface UseKeyboardShortcutsArgs {
  // Order of the currently visible models (after filter/sort) -
  // basis for left/right (plain list order) and the fallback for
  // up/down if the spatial search finds nothing.
  filteredIds: string[];
  selectedId: string | null;
  selectModel: (id: string) => void;
  hasBulkSelection: boolean;
  openBulkDeleteConfirm: () => void;
  // false while the detail page is open or another context is active
  // in which arrow key navigation in the grid makes no sense.
  navigationEnabled: boolean;
  // Space: add the selected model to the multi-selection.
  toggleBulkSelect: (id: string) => void;
}

// `?.scrollIntoView?.(...)` instead of `.scrollIntoView(...)`: both the element
// (keyboard navigation can hit an id whose tile isn't in the DOM right now,
// see the findSpatialNeighbor fallback) and the method itself
// (jsdom in tests doesn't implement scrollIntoView) can be missing.
function scrollTileIntoView(id: string): void {
  document.querySelector<HTMLElement>(`[${MODEL_TILE_ATTR}="${CSS.escape(id)}"]`)?.scrollIntoView?.({ block: 'nearest' });
}

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable;
}

// Tolerance in pixels within which two tile centers still count as the
// "same row" - absorbs subpixel rounding without wrongly merging real
// neighboring rows.
const ROW_TOLERANCE_PX = 4;

/**
 * Finds the next tile above/below by rendered position instead of a
 * column count: the grid wraps via `auto-fill`, and the folder view has
 * several mini grids. Takes the next row in that direction and the
 * horizontally closest tile in it; null if the current tile isn't in the
 * DOM or there is none.
 */
export function findSpatialNeighbor(
  container: ParentNode,
  currentId: string,
  direction: 'up' | 'down',
): string | null {
  const tiles = Array.from(container.querySelectorAll<HTMLElement>(`[${MODEL_TILE_ATTR}]`));
  const current = tiles.find((el) => el.getAttribute(MODEL_TILE_ATTR) === currentId);
  if (!current) return null;

  const currentRect = current.getBoundingClientRect();
  const currentCenterX = currentRect.left + currentRect.width / 2;
  const currentCenterY = currentRect.top + currentRect.height / 2;

  const candidates = tiles
    .filter((el) => el !== current)
    .map((el) => {
      const rect = el.getBoundingClientRect();
      return {
        id: el.getAttribute(MODEL_TILE_ATTR)!,
        centerX: rect.left + rect.width / 2,
        centerY: rect.top + rect.height / 2,
      };
    })
    .filter((c) => (direction === 'down' ? c.centerY > currentCenterY : c.centerY < currentCenterY));

  if (candidates.length === 0) return null;

  const minDistY = Math.min(...candidates.map((c) => Math.abs(c.centerY - currentCenterY)));
  const sameRow = candidates.filter((c) => Math.abs(Math.abs(c.centerY - currentCenterY) - minDistY) <= ROW_TOLERANCE_PX);
  sameRow.sort((a, b) => Math.abs(a.centerX - currentCenterX) - Math.abs(b.centerX - currentCenterX));
  return sameRow[0].id;
}

/**
 * Keyboard shortcuts of the catalog overview: "/" focuses the search, arrows
 * change the selection (up/down spatially), Space toggles the
 * multi-selection, Delete/Backspace opens the delete confirmation (doesn't
 * delete directly). Ignored while focus is in an input field.
 */
export function useKeyboardShortcuts({
  filteredIds,
  selectedId,
  selectModel,
  hasBulkSelection,
  openBulkDeleteConfirm,
  navigationEnabled,
  toggleBulkSelect,
}: UseKeyboardShortcutsArgs) {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (isTypingTarget(e.target)) return;

      if (e.key === '/') {
        e.preventDefault();
        document.getElementById(SEARCH_INPUT_ID)?.focus();
        return;
      }

      if ((e.key === 'Delete' || e.key === 'Backspace') && hasBulkSelection) {
        e.preventDefault();
        openBulkDeleteConfirm();
        return;
      }

      if (!navigationEnabled) return;

      if (e.key === ' ' && selectedId) {
        e.preventDefault();
        toggleBulkSelect(selectedId);
        return;
      }

      const isHorizontal = e.key === 'ArrowLeft' || e.key === 'ArrowRight';
      const isVertical = e.key === 'ArrowUp' || e.key === 'ArrowDown';
      if (!isHorizontal && !isVertical) return;
      if (filteredIds.length === 0) return;

      if (isVertical && selectedId) {
        const spatialTarget = findSpatialNeighbor(document, selectedId, e.key === 'ArrowDown' ? 'down' : 'up');
        if (spatialTarget) {
          e.preventDefault();
          selectModel(spatialTarget);
          scrollTileIntoView(spatialTarget);
          return;
        }
        // No spatial hit (e.g. collapsed folder): flat order.
      }

      const isNext = e.key === 'ArrowRight' || e.key === 'ArrowDown';
      const currentIndex = selectedId ? filteredIds.indexOf(selectedId) : -1;
      const nextIndex = currentIndex === -1 ? 0 : currentIndex + (isNext ? 1 : -1);
      if (nextIndex < 0 || nextIndex >= filteredIds.length) return;
      e.preventDefault();
      selectModel(filteredIds[nextIndex]);
      scrollTileIntoView(filteredIds[nextIndex]);
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [filteredIds, selectedId, selectModel, hasBulkSelection, openBulkDeleteConfirm, navigationEnabled, toggleBulkSelect]);
}
