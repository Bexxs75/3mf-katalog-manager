import { useEffect } from 'react';

export const SEARCH_INPUT_ID = 'catalog-search-input';

interface UseKeyboardShortcutsArgs {
  // Reihenfolge der aktuell sichtbaren Modelle (nach Filter/Sortierung) -
  // Grundlage fuer die Pfeiltasten-Navigation.
  filteredIds: string[];
  selectedId: string | null;
  selectModel: (id: string) => void;
  hasBulkSelection: boolean;
  openBulkDeleteConfirm: () => void;
  // false waehrend die Detailseite offen ist oder sonst ein Kontext aktiv
  // ist, in dem Pfeiltasten-Navigation im Raster keinen Sinn ergibt.
  navigationEnabled: boolean;
}

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable;
}

/**
 * Globale Tastaturkuerzel fuer die Katalog-Uebersicht: "/" fokussiert die
 * Suche, Pfeiltasten wechseln die Auswahl innerhalb der aktuell sichtbaren
 * Modell-Reihenfolge, Entf/Backspace oeffnet bei aktiver Mehrfachauswahl die
 * bestehende Loeschen-Bestaetigung (loescht NICHT direkt - dieselbe
 * Sicherheitsstufe wie der Button in der Bulk-Aktionsleiste). Alle drei
 * werden ignoriert, solange der Fokus in einem Eingabefeld liegt, damit
 * normales Tippen (auch ein "/" im Suchfeld selbst oder in einem
 * Tag-Eingabefeld) nicht beeintraechtigt wird.
 */
export function useKeyboardShortcuts({
  filteredIds,
  selectedId,
  selectModel,
  hasBulkSelection,
  openBulkDeleteConfirm,
  navigationEnabled,
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
      const isNext = e.key === 'ArrowRight' || e.key === 'ArrowDown';
      const isPrev = e.key === 'ArrowLeft' || e.key === 'ArrowUp';
      if (!isNext && !isPrev) return;
      if (filteredIds.length === 0) return;

      const currentIndex = selectedId ? filteredIds.indexOf(selectedId) : -1;
      const nextIndex = currentIndex === -1 ? 0 : currentIndex + (isNext ? 1 : -1);
      if (nextIndex < 0 || nextIndex >= filteredIds.length) return;
      e.preventDefault();
      selectModel(filteredIds[nextIndex]);
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [filteredIds, selectedId, selectModel, hasBulkSelection, openBulkDeleteConfirm, navigationEnabled]);
}
