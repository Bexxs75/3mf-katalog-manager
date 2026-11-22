import { CatalogSurfaceMenu, type SurfaceMenuEntry } from './CatalogSurfaceMenu';
import { useT } from '../i18n/LanguageContext';
import type { SortDirection, SortKey, ViewMode } from '../types';

export interface EmptyCatalogActions {
  onImport?: () => void;
  onSync?: () => void;
  onNewFolder?: () => void;
  onSelectAll?: () => void;
  onViewChange?: (view: ViewMode) => void;
  onSortChange?: (sort: SortKey) => void;
  onSortDirectionChange?: (direction: SortDirection) => void;
}
export function EmptyCatalogMenu({ x, y, onClose, view, sort, sortDirection = 'asc', busy, ...actions }: EmptyCatalogActions & {
  x: number; y: number; onClose: () => void; view: ViewMode; sort: SortKey; sortDirection?: SortDirection; busy: boolean;
}) {
  const t = useT();
  const entries: SurfaceMenuEntry[] = [];
  if (actions.onSync) entries.push({ label: t('emptyMenuSync'), shortcut: 'F5', action: actions.onSync, disabled: busy });
  if (actions.onImport) entries.push({ label: t('emptyMenuImport'), action: actions.onImport, disabled: busy });
  if (actions.onNewFolder) entries.push({ label: t('emptyMenuNewFolder'), action: actions.onNewFolder, disabled: busy });
  if (actions.onSelectAll) entries.push({ label: t('selectAllLabel'), action: actions.onSelectAll });
  if (actions.onViewChange) entries.push({ label: t('emptyMenuView'), children: (['grid', 'groupedGrid', 'groupedList'] as const).map((value, index) => ({
    label: t((['viewGrid', 'viewFolder', 'viewList'] as const)[index]), checked: view === value, action: () => actions.onViewChange?.(value),
  })) });
  const directionKeys = sort === 'name' ? ['sortNameAsc', 'sortNameDesc'] as const
    : sort === 'printStatus' ? ['sortPrintStatusAsc', 'sortPrintStatusDesc'] as const
    : sort === 'size' ? ['sortSizeAsc', 'sortSizeDesc'] as const
    : sort === 'vol' ? ['sortVolumeAsc', 'sortVolumeDesc'] as const : ['sortDateAsc', 'sortDateDesc'] as const;
  if (actions.onSortChange) entries.push({ label: t('sortLabel'), children: [
    ...(['name', 'imported', 'modified', 'size', 'vol', 'printStatus', 'viewed'] as const).map((value, index) => ({
      label: t((['sortName', 'sortImported', 'sortModified', 'sortSize', 'sortVolume', 'sortPrintStatus', 'sortLastViewed'] as const)[index]),
      checked: sort === value, action: () => actions.onSortChange?.(value),
    })),
    ...(actions.onSortDirectionChange ? [{ label: '', separator: true }] : []),
    ...(actions.onSortDirectionChange ? (['asc', 'desc'] as const).map((value, index) => ({
      label: t(directionKeys[index]), checked: sortDirection === value, action: () => actions.onSortDirectionChange?.(value),
    })) : []),
  ] });
  return <CatalogSurfaceMenu x={x} y={y} label={t('emptyMenuLabel')} entries={entries} onClose={onClose} />;
}
