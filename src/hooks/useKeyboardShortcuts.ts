import { shouldIgnoreCatalogShortcut, hasPlatformModifier, isMacPlatform } from '../lib/keyboardGuard';
import { useContext, useEffect } from 'react';
import { ModelLayoutContext, type ModelLayout } from './ModelLayoutContext';

export const SEARCH_INPUT_ID = 'catalog-search-input';
// Set by ModelGrid/ModelList on every model tile/row (see there) -
// the only coupling point between grid markup and this purely
// position-based navigation.
export const MODEL_TILE_ATTR = 'data-model-id';

interface UseKeyboardShortcutsArgs {
  // Order of the currently visible models (after filter/sort) -
  // basis for left/right (plain list order) and the fallback for
  // up/down if the spatial search finds nothing.
  onOpenDetail?: (id: string) => void;
  selectAllVisible?: () => void;
  onOpenTips?: () => void;
  singleKeyShortcuts?: boolean;
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
export function scrollTileIntoView(id: string, layout?: ModelLayout): void {
  if (layout) {
    layout.scrollToIndex(layout.order.indexOf(id));
    requestAnimationFrame(() => requestAnimationFrame(() => {
      document.querySelector<HTMLElement>(`[${MODEL_TILE_ATTR}="${CSS.escape(id)}"]`)?.scrollIntoView?.({ block: 'nearest' });
      document.querySelector<HTMLElement>(`[${MODEL_TILE_ATTR}="${CSS.escape(id)}"]`)?.focus({ preventScroll: true });
    }));
    return;
  }
  document.querySelector<HTMLElement>(`[${MODEL_TILE_ATTR}="${CSS.escape(id)}"]`)?.scrollIntoView?.({ block: 'nearest' });
  document.querySelector<HTMLElement>(`[${MODEL_TILE_ATTR}="${CSS.escape(id)}"]`)?.focus({ preventScroll: true });
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
  onOpenDetail, selectAllVisible, onOpenTips, singleKeyShortcuts = true,
  filteredIds,
  selectedId,
  selectModel,
  hasBulkSelection,
  openBulkDeleteConfirm,
  navigationEnabled,
  toggleBulkSelect,
}: UseKeyboardShortcutsArgs) {
  const registry = useContext(ModelLayoutContext);
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const platformModifier = hasPlatformModifier(e);
      const search = platformModifier && e.key.toLowerCase() === 'f';
      const selectAll = platformModifier && e.key.toLowerCase() === 'a';
      const macDelete = isMacPlatform() && platformModifier && e.key === 'Backspace';
      if (shouldIgnoreCatalogShortcut(e, { allowModifiers: search || selectAll || macDelete, allowInteractive: search })) return;
      if (search || (singleKeyShortcuts && e.key === '/')) {
        const input = document.getElementById(SEARCH_INPUT_ID) as HTMLInputElement | null;
        if (input) { e.preventDefault(); input.focus(); input.select(); }
        return;
      }
      if (singleKeyShortcuts && e.key === '?') { e.preventDefault(); onOpenTips?.(); return; }
      if (!navigationEnabled) return;
      if (selectAll) { e.preventDefault(); selectAllVisible?.(); return; }
      if (e.key === 'Enter' && selectedId) { e.preventDefault(); onOpenDetail?.(selectedId); return; }
      if ((macDelete || (singleKeyShortcuts && (e.key === 'Delete' || e.key === 'Backspace'))) && hasBulkSelection && !e.repeat) {
        e.preventDefault(); openBulkDeleteConfirm(); return;
      }

      if (singleKeyShortcuts && e.key === ' ' && selectedId) {
        e.preventDefault();
        toggleBulkSelect(selectedId);
        return;
      }

      const isHorizontal = e.key === 'ArrowLeft' || e.key === 'ArrowRight';
      const isVertical = e.key === 'ArrowUp' || e.key === 'ArrowDown';
      const isBoundary = e.key === 'Home' || e.key === 'End';
      if (!isHorizontal && !isVertical && !isBoundary) return;
      if (filteredIds.length === 0) return;

      const layouts = [...(registry?.layouts.values() ?? [])].sort((a, b) => (a.offsetTop ?? 0) - (b.offsetTop ?? 0));
      const layout = layouts.find(section => section.order.includes(selectedId ?? '')) ?? layouts[0];
      const order = layout ? layouts.flatMap(section => section.order) : filteredIds;
      if (isBoundary) {
        const id = order[e.key === 'Home' ? 0 : order.length - 1];
        e.preventDefault(); selectModel(id); scrollTileIntoView(id, layouts.find(section => section.order.includes(id))); return;
      }
      if (isVertical && selectedId) {
        const localIndex = layout?.order.indexOf(selectedId) ?? -1;
        const expectedId = layout?.order[localIndex + (e.key === 'ArrowDown' ? layout.columns : -layout.columns)];
        const missingNeighbor = expectedId && !document.querySelector(`[${MODEL_TILE_ATTR}="${CSS.escape(expectedId)}"]`);
        const spatialTarget = missingNeighbor ? null : findSpatialNeighbor(document, selectedId, e.key === 'ArrowDown' ? 'down' : 'up');
        if (spatialTarget) {
          e.preventDefault();
          selectModel(spatialTarget);
          scrollTileIntoView(spatialTarget, [...(registry?.layouts.values() ?? [])].find(section => section.order.includes(spatialTarget)));
          return;
        }
        // Offscreen rows use the measured column count.
      }

      const isNext = e.key === 'ArrowRight' || e.key === 'ArrowDown';
      const currentIndex = selectedId ? order.indexOf(selectedId) : -1;
      const step = isVertical ? layout?.columns ?? 1 : 1;
      const nextIndex = currentIndex === -1 ? 0 : currentIndex + (isNext ? step : -step);
      if (nextIndex < 0 || nextIndex >= order.length) return;
      e.preventDefault();
      selectModel(order[nextIndex]);
      scrollTileIntoView(order[nextIndex], [...(registry?.layouts.values() ?? [])].find(section => section.order.includes(order[nextIndex])));
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [onOpenDetail, selectAllVisible, onOpenTips, singleKeyShortcuts, registry, filteredIds, selectedId, selectModel, hasBulkSelection, openBulkDeleteConfirm, navigationEnabled, toggleBulkSelect]);
}
