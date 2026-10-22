import { useMemo, useRef, useState, type KeyboardEvent, type RefObject } from 'react';
import { useFormatCount, useT } from '../i18n/LanguageContext';
import { displayName } from '../lib/displayName';
import type { Folder } from '../types';
import { CatalogActionDialog, catalogActionBase, catalogActionButton } from './CatalogActionDialog';
import { Icon } from './Icon';

interface Row { folder: Folder; depth: number }

// Depth-first, so a child always follows its parent; the tree is always fully
// open because this is a picker, not navigation.
function flatten(folders: Folder[]): Row[] {
  const known = new Set(folders.map((f) => f.id));
  const children = new Map<string | null, Folder[]>();
  for (const folder of folders) {
    const parent = folder.parentId && known.has(folder.parentId) ? folder.parentId : null;
    children.set(parent, [...(children.get(parent) ?? []), folder]);
  }
  const rows: Row[] = [];
  const visit = (parent: string | null, depth: number) => {
    for (const folder of children.get(parent) ?? []) {
      rows.push({ folder, depth });
      visit(folder.id, depth + 1);
    }
  };
  visit(null, 0);
  return rows;
}

interface Props {
  count: number;
  folders: Folder[];
  onCancel: () => void;
  onMove: (folderId: string) => void;
  returnFocus?: RefObject<HTMLElement | null>;
}

/** "Move N models to ..." for use without a mouse: pick a folder with the arrow keys, Enter moves. */
export function MoveTargetDialog({ count, folders, onCancel, onMove, returnFocus }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  const rows = useMemo(() => flatten(folders), [folders]);
  // A folder that is missing on disk cannot receive files.
  const usable = rows.filter((row) => !row.folder.missing);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const treeRef = useRef<HTMLDivElement>(null);

  const select = (id: string) => {
    setSelectedId(id);
    treeRef.current?.querySelector<HTMLElement>(`[data-folder-id="${CSS.escape(id)}"]`)?.scrollIntoView?.({ block: 'nearest' });
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp' || event.key === 'Home' || event.key === 'End') {
      event.preventDefault();
      if (usable.length === 0) return;
      const current = usable.findIndex((row) => row.folder.id === selectedId);
      const next = event.key === 'Home' ? 0 : event.key === 'End' ? usable.length - 1
        : current < 0 ? (event.key === 'ArrowDown' ? 0 : usable.length - 1)
          : Math.max(0, Math.min(usable.length - 1, current + (event.key === 'ArrowDown' ? 1 : -1)));
      select(usable[next].folder.id);
    } else if (event.key === 'Enter' && selectedId) {
      event.preventDefault();
      onMove(selectedId);
    }
  };

  return (
    <CatalogActionDialog title={formatCount(t('moveDialogTitle'), count)} onClose={onCancel} returnFocus={returnFocus}>
      <p className="text-small text-[var(--ink-3)]">{t('moveDialogHint')}</p>
      {/* One tab stop with the arrow keys inside, like a tree widget; the active row follows the selection. */}
      <div ref={treeRef} data-initial-focus tabIndex={0} role="tree" aria-label={t('moveDialogTreeLabel')}
        aria-activedescendant={selectedId ? `move-target-${selectedId}` : undefined} onKeyDown={onKeyDown}
        className="max-h-[320px] overflow-auto rounded-[6px] border border-[var(--line)] p-1 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]">
        {rows.length === 0 && <p className="px-2 py-1.5 text-small text-[var(--ink-3)]">{t('moveDialogNoFolders')}</p>}
        {rows.map(({ folder, depth }) => (
          <div key={folder.id} id={`move-target-${folder.id}`} data-folder-id={folder.id} role="treeitem"
            aria-level={depth + 1} aria-selected={selectedId === folder.id} aria-disabled={folder.missing || undefined}
            title={folder.path}
            onClick={() => { if (!folder.missing) select(folder.id); }}
            onDoubleClick={() => { if (!folder.missing) onMove(folder.id); }}
            style={{ paddingLeft: 8 + depth * 16 }}
            className={`flex items-center gap-1.5 h-8 pr-2 rounded-[5px] text-small select-none ${folder.missing ? 'opacity-50 cursor-default' : 'cursor-pointer'} ${
              selectedId === folder.id ? 'bg-[var(--accent-soft)] text-[var(--accent)] font-semibold' : 'text-[var(--ink-2)] hover:bg-[var(--panel-2)]'}`}>
            <Icon name="folder" size={14} />
            <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{displayName(folder.name)}</span>
            <span className="font-medium tabular-nums text-caption text-[var(--ink-3)]">{folder.count}</span>
          </div>
        ))}
      </div>
      <div className="flex justify-end gap-2">
        <button type="button" className={catalogActionButton} onClick={onCancel}>{t('cancel')}</button>
        <button type="button" disabled={!selectedId} onClick={() => selectedId && onMove(selectedId)}
          className={`${catalogActionBase} border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)]`}>{t('moveDialogConfirm')}</button>
      </div>
    </CatalogActionDialog>
  );
}
