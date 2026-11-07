import type { ViewMode } from '../types';

const STORAGE_KEY = '3mf-katalog-view';

export function loadView(): ViewMode {
  try {
    const view = localStorage.getItem(STORAGE_KEY);
    return view === 'groupedGrid' || view === 'groupedList' ? view : 'grid';
  } catch {
    return 'grid';
  }
}

export function saveView(view: ViewMode): void {
  try {
    localStorage.setItem(STORAGE_KEY, view);
  } catch {
    // Storage blocked: the choice only applies to this session.
  }
}
