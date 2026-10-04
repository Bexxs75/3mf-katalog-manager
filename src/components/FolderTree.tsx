import { useImportLock } from '../hooks/ImportLockContext';
import type { useFolderExpansion } from '../hooks/useFolderExpansion';
import { FolderCatalogMenu } from './FolderCatalogMenu';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useDragThreshold } from '../hooks/useDragThreshold';
import type { Folder } from '../types';
import { useT } from '../i18n/LanguageContext';

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
  draggedFileId?: string | null;
  draggedFileFolderId?: string | null;
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
  draggedFileId = null,
  draggedFileFolderId = null,
  dragOverFolderId = null,
  draggedFolderId = null,
  onFolderMouseEnter,
  onFolderMouseLeave,
  onDragFolderStart,
}: Props) {
  const { jobActive } = useImportLock();
  const t = useT();
  const [menu, setMenu] = useState<{ id: string; name: string; x: number; y: number } | null>(null);
  const returnFocus = useRef<HTMLElement | null>(null);
  const allModelsRef = useRef<HTMLDivElement>(null);
  const tree = useMemo(() => buildTree(folders), [folders]);

  // Folder rows are only 28px high: an immediate drag start would let even a
  // slight slip during a click trigger a real move_folder.
  const folderDrag = useDragThreshold(jobActive ? undefined : onDragFolderStart);

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
    const validFileTarget = !!draggedFileId && node.id !== draggedFileFolderId;
    const isDraggedOver = dragOverFolderId === node.id && (!draggedFileId || validFileTarget);
    return (
      <div key={node.id}>
        <div
          tabIndex={0}
          role="button"
          onContextMenu={(e) => { e.preventDefault(); returnFocus.current = e.currentTarget; setMenu({ id: node.id, name: node.name, x: e.clientX, y: e.clientY }); }}
          onKeyDown={(e) => {
            if (e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10')) {
              e.preventDefault(); e.stopPropagation(); returnFocus.current = e.currentTarget;
              const rect = e.currentTarget.getBoundingClientRect();
              setMenu({ id: node.id, name: node.name, x: rect.left, y: rect.bottom });
            } else if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); e.stopPropagation(); onSelect(node.id); }
          }}
          onClick={() => onSelect(node.id)}
          onMouseDown={(e) => folderDrag.begin(e, node.id)}
          onMouseEnter={() => { setHoveredId(node.id); onFolderMouseEnter?.(node.id); }}
          onMouseLeave={() => { setHoveredId((current) => current === node.id ? null : current); onFolderMouseLeave?.(node.id); }}
          style={{ paddingLeft: 6 + depth * 16 }}
          className={`flex items-center gap-1.5 h-7 pr-2 rounded-[7px] cursor-pointer select-none text-[12.5px] border ${
            node.id === activeFolderId
              ? 'bg-[var(--accent-soft)] text-[var(--accent)] font-semibold'
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
            className={`w-3.5 text-[9px] text-[var(--ink-3)] ${node.children.length === 0 ? 'invisible' : ''}`}
          >
            {isOpen ? '▾' : '▸'}
          </span>
          <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{node.name}</span>
          <span className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">{node.count}</span>
          {isDraggedOver && <span className="font-mono-ui text-[10px] text-[var(--accent)]">{t('dropHereLabel')}</span>}
        </div>
        {isOpen && node.children.map((c) => renderNode(c, depth + 1))}
      </div>
    );
  }

  return (
    <div>
      <div
        ref={allModelsRef}
        tabIndex={0}
        role="button"
        onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); e.stopPropagation(); onSelect('all'); } }}
        onClick={() => onSelect('all')}
        className={`flex items-center gap-2 h-7 px-1.5 rounded-[7px] cursor-pointer text-[12.5px] ${
          activeFolderId === 'all'
            ? 'bg-[var(--accent-soft)] text-[var(--accent)] font-semibold'
            : 'text-[var(--ink-2)] hover:bg-[var(--panel-2)] hover:text-[var(--ink)]'
        }`}
      >
        <span className="flex-1">{t('allModelsLabel')}</span>
        <span className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">{totalModelCount}</span>
      </div>
      {tree.map((n) => renderNode(n, 0))}
      {menu && <FolderCatalogMenu folderId={menu.id} name={menu.name} x={menu.x} y={menu.y}
        returnFocus={returnFocus} onClose={() => setMenu(null)} onRemoved={() => {
          let active = folders.find((f) => f.id === activeFolderId);
          const seen = new Set<string>();
          while (active && !seen.has(active.id)) {
            if (active.id === menu.id) { onSelect('all'); break; }
            seen.add(active.id); active = folders.find((f) => f.id === active?.parentId);
          }
          returnFocus.current = allModelsRef.current;
          allModelsRef.current?.focus();
          onRemoved?.();
        }} /> }
    </div>
  );
}
