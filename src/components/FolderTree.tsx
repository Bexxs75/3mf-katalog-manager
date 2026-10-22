import { isReadOnly } from '../lib/readOnly';
import { canManageFiles, useCanManageFiles } from '../lib/webSession';
import { displayName } from '../lib/displayName';
import { Icon } from './Icon';
import { useImportLock } from '../hooks/ImportLockContext';
import type { useFolderExpansion } from '../hooks/useFolderExpansion';
import { FolderCatalogMenu } from './FolderCatalogMenu';
import { useEffect, useMemo, useRef, useState } from 'react';
import { renameFolder } from '../lib/api/folders';
import { expectedError, toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { useDragThreshold } from '../hooks/useDragThreshold';
import type { Folder, FolderTrashResult } from '../types';
import { useT, useFormatCount } from '../i18n/LanguageContext';

interface TreeNode extends Folder {
  children: TreeNode[];
}

interface Props {
  expansion: ReturnType<typeof useFolderExpansion>;
  folders: Folder[];
  totalModelCount: number;
  activeFolderId: string;
  onSelect: (id: string) => void;
  onRemoved?: () => void;
  onTrashed?: (result: FolderTrashResult) => void;
  onRenamed?: (info: { id: string; from: string; to: string; count: number }) => void;
  draggedFileId?: string | null;
  draggedFileFolderId?: string | null;
  /** Number of cards travelling with the dragged one; more than one changes the drop label. */
  draggedFileCount?: number;
  dragOverFolderId?: string | null;
  draggedFolderId?: string | null;
  onFolderMouseEnter?: (id: string) => void;
  onFolderMouseLeave?: (id: string) => void;
  onDragFolderStart?: (id: string) => void;
}

function buildTree(folders: Folder[]): TreeNode[] {
  const byId = new Map<string, TreeNode>(folders.map((f) => [f.id, { ...f, children: [] }]));
  const roots: TreeNode[] = [];
  for (const node of byId.values()) {
    if (node.parentId && byId.has(node.parentId)) {
      byId.get(node.parentId)!.children.push(node);
    } else {
      roots.push(node);
    }
  }
  return roots;
}

export function FolderTree({
  expansion,
  folders,
  totalModelCount,
  activeFolderId,
  onSelect,
  onRemoved,
  onTrashed,
  onRenamed,
  draggedFileId = null,
  draggedFileFolderId = null,
  draggedFileCount = 1,
  dragOverFolderId = null,
  draggedFolderId = null,
  onFolderMouseEnter,
  onFolderMouseLeave,
  onDragFolderStart,
}: Props) {
  const canManage = useCanManageFiles();
  const { jobActive } = useImportLock();
  const t = useT();
  const formatCount = useFormatCount();
  const [menu, setMenu] = useState<{ id: string; name: string; x: number; y: number } | null>(null);
  const returnFocus = useRef<HTMLElement | null>(null);
  const allModelsRef = useRef<HTMLDivElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const [renaming, setRenaming] = useState<{ id: string; draft: string; error: AppError | null; busy: boolean } | null>(null);
  const editRef = useRef<HTMLDivElement>(null);
  // Set when the row should get the focus back once the edit field is gone.
  const focusRowAfterRename = useRef<string | null>(null);
  const tree = useMemo(() => buildTree(folders), [folders]);

  // Folder rows are only 28px high: an immediate drag start would let even a
  // slight slip during a click trigger a real move_folder.
  const folderDrag = useDragThreshold(jobActive || !canManage ? undefined : onDragFolderStart);

  // The folder row disappears: if the active folder lay inside it, fall back to "All models" and park the focus there.
  function leaveFolder(folderId: string) {
    let active = folders.find((f) => f.id === activeFolderId);
    const seen = new Set<string>();
    while (active && !seen.has(active.id)) {
      if (active.id === folderId) { onSelect('all'); break; }
      seen.add(active.id); active = folders.find((f) => f.id === active?.parentId);
    }
    returnFocus.current = allModelsRef.current;
    allModelsRef.current?.focus();
  }

  function startRename(node: Folder) {
    if (jobActive || !canManageFiles()) return;
    setRenaming({ id: node.id, draft: node.name, error: null, busy: false });
  }
  function cancelRename(restoreFocus: boolean) {
    if (restoreFocus && renaming) focusRowAfterRename.current = renaming.id;
    setRenaming(null);
  }
  async function commitRename(node: Folder) {
    if (!renaming || renaming.busy) return;
    const name = renaming.draft.trim();
    if (name === node.name) { cancelRename(true); return; }
    const invalid = !name ? t('renameFolderEmpty') : /[/\\]/.test(name) ? t('renameFolderSeparator') : null;
    if (invalid) { setRenaming({ ...renaming, error: expectedError(invalid) }); return; }
    setRenaming({ ...renaming, error: null, busy: true });
    try {
      await renameFolder(node.id, name);
    } catch (e) {
      // Keep the field open with the typed text so the name can be corrected.
      setRenaming((current) => current && current.id === node.id ? { ...current, error: toAppError(e), busy: false } : current);
      return;
    }
    focusRowAfterRename.current = node.id;
    setRenaming(null);
    onRenamed?.({ id: node.id, from: node.name, to: name, count: node.count });
  }
  // Focus moves into the field when editing starts and back to the row when it ends.
  const renamingId = renaming?.id ?? null;
  useEffect(() => {
    if (renamingId) {
      const input = editRef.current?.querySelector('input');
      input?.focus(); input?.select();
    } else if (focusRowAfterRename.current) {
      const id = focusRowAfterRename.current;
      focusRowAfterRename.current = null;
      rootRef.current?.querySelector<HTMLElement>(`[data-sync-folder-id="${CSS.escape(id)}"]`)?.focus();
    }
  }, [renamingId]);
  useEffect(() => {
    if (!renamingId) return;
    const outside = (e: MouseEvent) => {
      if (!editRef.current?.contains(e.target as Node)) setRenaming((current) => current?.busy ? current : null);
    };
    document.addEventListener('mousedown', outside);
    return () => document.removeEventListener('mousedown', outside);
  }, [renamingId]);

  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const { isExpanded, expand } = expansion;
  useEffect(() => {
    if ((!draggedFileId && !draggedFolderId) || !hoveredId || isExpanded(hoveredId) || !folders.some((folder) => folder.parentId === hoveredId)) return;
    const timer = window.setTimeout(() => expand(hoveredId), 600);
    const cancel = () => { window.clearTimeout(timer); setHoveredId(null); };
    window.addEventListener('mouseup', cancel);
    return () => { window.clearTimeout(timer); window.removeEventListener('mouseup', cancel); };
  }, [hoveredId, draggedFileId, draggedFolderId, folders, isExpanded, expand]);

  function renderNode(node: TreeNode, depth: number) {
    const isOpen = expansion.isExpanded(node.id);
    const validFileTarget = canManage && !!draggedFileId && node.id !== draggedFileFolderId;
    const isDraggedOver = canManage && dragOverFolderId === node.id && (!draggedFileId || validFileTarget);
    if (renaming?.id === node.id) {
      const errorId = `rename-error-${node.id}`;
      const hintId = `rename-hint-${node.id}`;
      return (
        <div key={node.id}>
          <div ref={editRef} data-sync-folder-id={node.id} style={{ paddingLeft: 6 + depth * 16 }} className="pr-2">
            <div className="flex items-center gap-1.5 h-7">
              <span className="w-3.5 invisible"><Icon name="chevron" size={14} /></span>
              <input
                value={renaming.draft}
                // readOnly instead of disabled: a disabled input would drop the focus.
                readOnly={renaming.busy}
                aria-label={t('renameFolderLabel')}
                aria-invalid={renaming.error ? true : undefined}
                aria-describedby={renaming.error ? `${errorId} ${hintId}` : hintId}
                onChange={(e) => setRenaming({ ...renaming, draft: e.target.value, error: null })}
                onKeyDown={(e) => {
                  e.stopPropagation();
                  if (e.key === 'Enter') { e.preventDefault(); void commitRename(node); }
                  else if (e.key === 'Escape') { e.preventDefault(); cancelRename(true); }
                }}
                onBlur={(e) => {
                  // Tabbing out cancels; a null relatedTarget (window switch, click) is left to the mousedown handler.
                  if (e.relatedTarget && !editRef.current?.contains(e.relatedTarget as Node)) cancelRename(false);
                }}
                className={`flex-1 min-w-0 h-6 px-1.5 rounded-[5px] border bg-[var(--panel)] text-[var(--ink)] text-small outline-none focus:shadow-[0_0_0_2px_var(--accent-soft)] ${renaming.error ? 'border-red-400' : 'border-[var(--accent)]'}`}
              />
            </div>
            <p id={hintId} className="pl-5 pb-1 text-caption text-[var(--ink-3)]">{formatCount(t('renameFolderHint'), node.count)}</p>
            {renaming.error && <p id={errorId} role="alert" className="pl-5 pb-1 text-caption text-red-400"><ErrorText error={renaming.error} /></p>}
          </div>
          {isOpen && node.children.map((c) => renderNode(c, depth + 1))}
        </div>
      );
    }
    return (
      <div key={node.id}>
        <div data-sync-folder-id={node.id}
          tabIndex={0}
          role="button"
          onContextMenu={(e) => { e.preventDefault(); if (!canManageFiles()) return; returnFocus.current = e.currentTarget; setMenu({ id: node.id, name: node.name, x: e.clientX, y: e.clientY }); }}
          onKeyDown={(e) => {
            if (canManageFiles() && (e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10'))) {
              e.preventDefault(); e.stopPropagation(); returnFocus.current = e.currentTarget;
              const rect = e.currentTarget.getBoundingClientRect();
              setMenu({ id: node.id, name: node.name, x: rect.left, y: rect.bottom });
            } else if (e.key === 'F2') {
              e.preventDefault(); e.stopPropagation(); startRename(node);
            } else if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); e.stopPropagation(); onSelect(node.id); }
          }}
          onClick={() => onSelect(node.id)}
          onMouseDown={(e) => folderDrag.begin(e, node.id)}
          onMouseEnter={() => { setHoveredId(node.id); onFolderMouseEnter?.(node.id); }}
          onMouseLeave={() => { setHoveredId((current) => current === node.id ? null : current); onFolderMouseLeave?.(node.id); }}
          style={{ paddingLeft: 6 + depth * 16 }}
          title={node.missing ? t('folderMissingOnDisk') : undefined}
          className={`flex items-center gap-1.5 h-7 pr-2 rounded-[7px] cursor-pointer select-none text-small border ${
            node.id === activeFolderId
              ? 'bg-[var(--accent-soft)] text-[var(--accent)] font-semibold hover:text-[var(--ink)]'
              : 'text-[var(--ink-2)] hover:bg-[var(--panel-2)] hover:text-[var(--ink)]'
          } ${
            isDraggedOver ? 'border-[var(--accent)] bg-[var(--accent-soft)] shadow-[0_0_0_2px_var(--accent-soft)]' : validFileTarget ? 'border-dashed border-[var(--line-strong)]' : 'border-transparent'
          } ${
            draggedFolderId === node.id ? 'opacity-40' : ''
          }`}
        >
          <span
            onClick={(e) => {
              if (node.children.length === 0) return;
              e.stopPropagation();
              expansion.toggle(node.id);
            }}
            className={`hover:bg-[var(--panel-2)] hover:text-[var(--ink)] w-3.5 text-compact-label text-[var(--ink-3)] ${node.children.length === 0 ? 'invisible' : ''}`}
          >
            <Icon name="chevron" size={14} className={isOpen  ? '' : '-rotate-90'} />
          </span>
          {node.missing && <span className="text-[var(--warn)] shrink-0 flex" data-testid="folder-missing-icon"><Icon name="warning" size={14} /></span>}
          <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{displayName(node.name)}</span>
          {node.missing && <span className="shrink-0 rounded-full px-1.5 text-compact-meta bg-[var(--warn-soft)] text-[var(--warn)]">{t('folderMissingOnDisk')}</span>}
          <span className="font-medium tabular-nums text-caption text-[var(--ink-3)]">{node.count}</span>
          {isDraggedOver && <span className="font-medium tabular-nums text-compact-meta text-[var(--accent)]">{draggedFileId && draggedFileCount > 1 ? formatCount(t('dropHereMany'), draggedFileCount) : t('dropHereLabel')}</span>}
        </div>
        {isOpen && node.children.map((c) => renderNode(c, depth + 1))}
      </div>
    );
  }

  return (
    <div ref={rootRef}>
      <div
        ref={allModelsRef}
        tabIndex={0}
        role="button"
        onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); e.stopPropagation(); onSelect('all'); } }}
        onClick={() => onSelect('all')}
        className={`flex items-center gap-2 h-7 px-1.5 rounded-[7px] cursor-pointer text-small ${
          activeFolderId === 'all'
            ? 'bg-[var(--accent-soft)] text-[var(--accent)] font-semibold hover:text-[var(--ink)]'
            : 'text-[var(--ink-2)] hover:bg-[var(--panel-2)] hover:text-[var(--ink)]'
        }`}
      >
        <span className="flex-1">{t('allModelsLabel')}</span>
        <span className="font-medium tabular-nums text-caption text-[var(--ink-3)]">{totalModelCount}</span>
      </div>
      {tree.map((n) => renderNode(n, 0))}
      {menu && canManage && <FolderCatalogMenu folders={folders} folderId={menu.id} name={menu.name} x={menu.x} y={menu.y}
        returnFocus={returnFocus} onClose={() => setMenu(null)}
        onRename={() => { const node = folders.find((f) => f.id === menu.id); setMenu(null); if (node) startRename(node); }}
        onRemoved={() => { leaveFolder(menu.id); if (!isReadOnly()) onRemoved?.(); }}
        onTrashed={(result) => { leaveFolder(menu.id); onTrashed?.(result); }} /> }
    </div>
  );
}
