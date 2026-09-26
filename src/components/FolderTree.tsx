import { useMemo, useState } from 'react';
import { useDragThreshold } from '../hooks/useDragThreshold';
import type { Folder } from '../types';
import { useT } from '../i18n/LanguageContext';

interface TreeNode extends Folder {
  children: TreeNode[];
}

interface Props {
  folders: Folder[];
  totalModelCount: number;
  activeFolderId: string;
  onSelect: (id: string) => void;
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
  folders,
  totalModelCount,
  activeFolderId,
  onSelect,
  dragOverFolderId = null,
  draggedFolderId = null,
  onFolderMouseEnter,
  onFolderMouseLeave,
  onDragFolderStart,
}: Props) {
  const t = useT();
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const tree = useMemo(() => buildTree(folders), [folders]);

  const toggle = (id: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });

  // Folder rows are only 28px high: an immediate drag start would let even a
  // slight slip during a click trigger a real move_folder.
  const folderDrag = useDragThreshold(onDragFolderStart);

  function renderNode(node: TreeNode, depth: number) {
    const isOpen = expanded.has(node.id);
    return (
      <div key={node.id}>
        <div
          onClick={() => onSelect(node.id)}
          onMouseDown={(e) => folderDrag.begin(e, node.id)}
          onMouseEnter={() => onFolderMouseEnter?.(node.id)}
          onMouseLeave={() => onFolderMouseLeave?.(node.id)}
          style={{ paddingLeft: 6 + depth * 16 }}
          className={`flex items-center gap-1.5 h-7 pr-2 rounded-[7px] cursor-pointer select-none text-[12.5px] border ${
            node.id === activeFolderId
              ? 'bg-[var(--accent-soft)] text-[var(--accent)] font-semibold border-transparent'
              : 'text-[var(--ink-2)] hover:bg-[var(--panel-2)] hover:text-[var(--ink)] border-transparent'
          } ${
            dragOverFolderId === node.id ? 'border-[var(--accent)] bg-[var(--accent-soft)]' : ''
          } ${
            draggedFolderId === node.id ? 'opacity-40' : ''
          }`}
        >
          <span
            onClick={(e) => {
              if (node.children.length === 0) return;
              e.stopPropagation();
              toggle(node.id);
            }}
            className={`w-3.5 text-[9px] text-[var(--ink-3)] ${node.children.length === 0 ? 'invisible' : ''}`}
          >
            {isOpen ? '▾' : '▸'}
          </span>
          <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{node.name}</span>
          <span className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">{node.count}</span>
        </div>
        {isOpen && node.children.map((c) => renderNode(c, depth + 1))}
      </div>
    );
  }

  return (
    <div>
      <div
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
    </div>
  );
}
