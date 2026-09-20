import { useEffect } from 'react';

export const SEARCH_INPUT_ID = 'catalog-search-input';
// Von ModelGrid/ModelList auf jeder Modell-Kachel/-Zeile gesetzt (siehe dort) -
// einziger Kopplungspunkt zwischen Raster-Markup und dieser rein positions-
// basierten Navigation.
export const MODEL_TILE_ATTR = 'data-model-id';

interface UseKeyboardShortcutsArgs {
  // Reihenfolge der aktuell sichtbaren Modelle (nach Filter/Sortierung) -
  // Grundlage fuer Links/Rechts (einfache Listen-Reihenfolge) und den
  // Fallback fuer Hoch/Runter, falls die räumliche Suche nichts findet.
  filteredIds: string[];
  selectedId: string | null;
  selectModel: (id: string) => void;
  hasBulkSelection: boolean;
  openBulkDeleteConfirm: () => void;
  // false waehrend die Detailseite offen ist oder sonst ein Kontext aktiv
  // ist, in dem Pfeiltasten-Navigation im Raster keinen Sinn ergibt.
  navigationEnabled: boolean;
  // Schaltet die Bulk-Auswahl (Checkbox) fuer eine einzelne Datei um -
  // von der Leertaste genutzt, um das gerade per Pfeiltasten/Klick
  // ausgewaehlte Modell in die Mehrfachauswahl aufzunehmen, ohne die Maus
  // auf die kleine Checkbox in der Kachel-Ecke zielen zu muessen.
  toggleBulkSelect: (id: string) => void;
}

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable;
}

// Toleranz in Pixeln, innerhalb derer zwei Kachel-Mittelpunkte noch als
// "gleiche Zeile" gelten - faengt Subpixel-Rundung ab, ohne echte
// Nachbarzeilen faelschlich zusammenzufassen.
const ROW_TOLERANCE_PX = 4;

/**
 * Findet die naechste Kachel oberhalb/unterhalb der aktuell ausgewaehlten,
 * anhand ihrer TATSAECHLICH gerenderten Position (getBoundingClientRect) statt
 * anhand einer angenommenen Spaltenzahl. Das ist notwendig, weil das Raster
 * responsiv per CSS Grid `auto-fill` umbricht (Spaltenzahl haengt von der
 * Fensterbreite ab) und in der Ordner-Ansicht mehrere unabhaengige Mini-Raster
 * (ein Grid pro Ordner-Abschnitt) nebeneinander existieren.
 *
 * Naeherung: alle Kacheln in der angefragten Richtung (Mittelpunkt-Y echt
 * kleiner/groesser als die aktuelle), davon die mit dem geringsten
 * Y-Abstand (= naechste Zeile), und darunter die mit dem geringsten
 * X-Abstand (= gleiche Spalte bzw. naeheste). Gibt null zurueck, wenn die
 * aktuelle Kachel nicht im DOM ist (z.B. in eingeklapptem Ordner) oder keine
 * Kachel in der gesuchten Richtung existiert - der Aufrufer faellt dann auf
 * die flache Listen-Reihenfolge zurueck.
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
 * Globale Tastaturkuerzel fuer die Katalog-Uebersicht: "/" fokussiert die
 * Suche, Pfeiltasten wechseln die Auswahl (Links/Rechts entlang der Listen-
 * Reihenfolge, Hoch/Runter raeumlich zur naechsten Zeile - siehe
 * findSpatialNeighbor), Leertaste schaltet die Bulk-Auswahl-Checkbox des
 * aktuell ausgewaehlten Modells um, Entf/Backspace oeffnet bei aktiver
 * Mehrfachauswahl die bestehende Loeschen-Bestaetigung (loescht NICHT
 * direkt - dieselbe Sicherheitsstufe wie der Button in der Bulk-
 * Aktionsleiste). Alle vier werden ignoriert, solange der Fokus in einem
 * Eingabefeld liegt, damit normales Tippen (auch ein "/" oder eine
 * Leertaste im Suchfeld selbst oder in einem Tag-Eingabefeld) nicht
 * beeintraechtigt wird.
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
          return;
        }
        // Kein raeumlicher Treffer (z.B. Auswahl gerade nicht im DOM, weil ihr
        // Ordner-Abschnitt eingeklappt ist) - auf die flache Reihenfolge
        // zurueckfallen, statt gar nichts zu tun.
      }

      const isNext = e.key === 'ArrowRight' || e.key === 'ArrowDown';
      const currentIndex = selectedId ? filteredIds.indexOf(selectedId) : -1;
      const nextIndex = currentIndex === -1 ? 0 : currentIndex + (isNext ? 1 : -1);
      if (nextIndex < 0 || nextIndex >= filteredIds.length) return;
      e.preventDefault();
      selectModel(filteredIds[nextIndex]);
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [filteredIds, selectedId, selectModel, hasBulkSelection, openBulkDeleteConfirm, navigationEnabled, toggleBulkSelect]);
}
