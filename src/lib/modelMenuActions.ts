import type { Translations } from '../i18n/types';
import type { ModelFile } from '../types';

type LabelKey = { [K in keyof Translations]: Translations[K] extends string ? K : never }[keyof Translations];
export type ModelMenuActionId = 'dateRename' | 'rereadPrintData' | 'editBundle' | 'dissolveBundle' | 'favorite' | 'queue' | 'printed' | 'slicer' | 'reveal' | 'download' | 'stl' | 'plates' | 'addTag' | 'removeTag' | 'collection' | 'uncollect' | 'license' | 'bundle' | 'addBundle' | 'unbundle' | 'rename' | 'move' | 'remove' | 'trash';
export interface ModelMenuAction {
  id: ModelMenuActionId;
  label: LabelKey;
  group: 'quick' | 'files' | 'organize' | 'bundle' | 'end';
  shortcut?: string;
  shortcutLabel?: LabelKey;
  checked?: boolean | 'mixed';
}
export function selectionState(values: boolean[]): boolean | 'mixed' {
  return values.every(Boolean) ? true : values.some(Boolean) ? 'mixed' : false;
}
export function contextSelection(models: ModelFile[], clickedId: string, selected: ReadonlySet<string>): ModelFile[] {
  return models.filter(model => selected.has(clickedId) ? selected.has(model.id) : model.id === clickedId);
}
/** Visit every selected file even if an earlier operation fails. */
export async function runModelSelection<T>(models: readonly T[], action: (model: T) => unknown) {
  const failures: unknown[] = [];
  for (const model of models) {
    try { await action(model); } catch (error) { failures.push(error); }
  }
  if (failures.length) throw failures[0];
}
export function modelMenuActions(options: { toolbar?: boolean; web: boolean; count: number; favorite: boolean | 'mixed'; queue: boolean | 'mixed'; printed: boolean | 'mixed' }): ModelMenuAction[] {
  const actions: ModelMenuAction[] = [
    { id: 'favorite', group: 'quick', label: options.favorite === true ? 'favoriteRemove' : 'favoriteAdd', checked: options.favorite },
    { id: 'queue', group: 'quick', label: options.queue === true ? 'removeFromQueue' : 'addToQueue', checked: options.queue },
    { id: 'printed', group: 'quick', label: options.printed === true ? 'markAsNotPrinted' : 'markAsPrinted', checked: options.printed },
    ...(options.web ? [{ id: 'download', group: 'files', label: 'downloadModel' } as const] : [
      { id: 'slicer', group: 'files', label: 'openInSlicer' } as const,
      { id: 'reveal', group: 'files', label: 'showInFileManager' } as const,
      { id: 'stl', group: 'files', label: 'stSaveAsStl' } as const,
    ]),
    { id: 'plates', group: 'files', label: 'poOpen' },
    { id: 'addTag', group: 'organize', label: 'bulkAddTagLabel' },
    { id: 'removeTag', group: 'organize', label: 'bulkRemoveTagLabel' },
    { id: 'collection', group: 'organize', label: 'addToCollectionLabel' },
    { id: 'uncollect', group: 'organize', label: 'removeFromCollectionLabel' },
    { id: 'license', group: 'organize', label: 'licenseSet' },
    { id: 'bundle', group: 'bundle', label: 'bundleMake' },
    { id: 'addBundle', group: 'bundle', label: 'bundleAddTo' },
    { id: 'unbundle', group: 'bundle', label: 'bundleRemoveFrom' },
    ...(options.count === 1 ? [{ id: 'rename', group: 'end', label: 'renameLabel', shortcut: 'F2' } as const] : []),
    { id: 'move', group: 'end', label: 'moveContextLabel', shortcut: 'F6' },
    { id: 'remove', group: 'end', label: 'removeCatalog', shortcut: 'Delete', shortcutLabel: 'modelMenuDeleteKey' },
    ...(options.toolbar ? [
      { id: 'dateRename', group: 'organize', label: 'dateRenameMenu' } as const,
      { id: 'rereadPrintData', group: 'organize', label: 'printDataRereadSelected' } as const,
      { id: 'editBundle', group: 'bundle', label: 'bundleEdit' } as const,
      { id: 'dissolveBundle', group: 'bundle', label: 'bundleDissolve' } as const,
    ] : []),
    { id: 'trash', group: 'end', label: 'modelMenuTrash', shortcut: 'Shift+Delete', shortcutLabel: 'modelMenuTrashKey' },
  ];
  return options.toolbar ? actions.map(action => ['move', 'remove'].includes(action.id) ? { ...action, group: 'organize' } : action) : actions;
}
