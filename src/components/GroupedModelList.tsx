import { useMemo, type RefObject } from 'react';
import { useDragThreshold } from '../hooks/useDragThreshold';
import type { ModelFile, Folder } from '../types';
import { buildGroupedFolderTree, type GroupedFolderNode } from '../lib/groupedFolderTree';
import { ModelList } from './ModelList';
import { useT } from '../i18n/LanguageContext';
import { NO_FOLDER_COLLAPSE_KEY, type useCollapsedFolders } from '../hooks/useCollapsedFolders';

interface Props {
  models: ModelFile[];
  containerRef?: RefObject<HTMLDivElement | null>;
  folders: Folder[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onOpenDetail: (id: string) => void;
  onContextMenu: (id: string, x: number, y: number) => void;
  selectedForBulk: Set<string>;
  onToggleBulkSelect: (id: string) => void;
  onDragFileStart?: (id: string) => void;
  draggedFileId?: string | null;
  draggedFileFolderId?: string | null;
  draggedFolderId: string | null;
  dragOverFolderId: string | null;
  onDragFolderStart: (id: string) => void;
  onFolderMouseEnter: (id: string) => void;
  onFolderMouseLeave: (id: string) => void;
  collapsedFolders: ReturnType<typeof useCollapsedFolders>;
}


export function GroupedModelList({
  models,
  folders,
  draggedFileId = null,
  draggedFileFolderId = null,
  draggedFolderId,
  dragOverFolderId,
  onDragFolderStart,
  onFolderMouseEnter,
  onFolderMouseLeave,
  collapsedFolders,
  ...modelListProps
}: Props) {
  const t = useT();
  const { roots, noFolder } = useMemo(() => buildGroupedFolderTree(folders, models), [folders, models]);

  const folderDrag = useDragThreshold(onDragFolderStart);

  function renderNode(node: GroupedFolderNode, depth: number) {
    // totalCount is recursive: 0 means the whole subtree contains nothing under
    // the filter (otherwise a wall of empty header rows).
    if (node.totalCount === 0 && !draggedFileId) return null;
    const isCollapsed = collapsedFolders.isCollapsed(node.folder.id);
    const validFileTarget = !!draggedFileId && node.folder.id !== draggedFileFolderId;
    const isDraggedOver = dragOverFolderId === node.folder.id && (!draggedFileId || validFileTarget);
    const isBeingDragged = draggedFolderId === node.folder.id;
    return (
      <div key={node.folder.id} className={depth > 0 ? 'ml-3 pl-4 border-l border-[var(--line-strong)] mt-2.5' : 'mb-4'}>
        <div
          onMouseDown={(e) => folderDrag.begin(e, node.folder.id)}
          onMouseEnter={() => onFolderMouseEnter(node.folder.id)}
          onMouseLeave={() => onFolderMouseLeave(node.folder.id)}
          onClick={() => collapsedFolders.toggle(node.folder.id)}
          className={`flex items-center gap-2 py-1.5 cursor-pointer select-none rounded-[4px] ${
            isDraggedOver ? 'bg-[var(--accent-soft)] border border-[var(--accent)] shadow-[0_0_0_2px_var(--accent-soft)]' : validFileTarget ? 'border border-dashed border-[var(--line-strong)]' : 'border border-transparent'
          } ${isBeingDragged ? 'opacity-40' : ''}`}
        >
          <span className={`text-[9px] text-[var(--ink-3)] transition-transform ${isCollapsed ? '-rotate-90' : ''}`}>▾</span>
          <span className="text-[13px]">📁</span>
          <span className="text-[13px] font-semibold">{node.folder.name}</span>
          <span className="font-mono-ui text-[10px] text-[var(--ink-3)]">{node.totalCount}</span>
          <span className="flex-1 h-px bg-[var(--line)]" />
          {isDraggedOver && <span className="font-mono-ui text-[10px] text-[var(--accent)]">{t('dropHereLabel')}</span>}
        </div>
        {!isCollapsed && (
          <div className="mt-2">
            {node.children.map((child) => renderNode(child, depth + 1))}
            {node.files.length > 0 && <ModelList windowed={node.files.length > 60} models={node.files} {...modelListProps} />}
          </div>
        )}
      </div>
    );
  }

  const noFolderCollapsed = collapsedFolders.isCollapsed(NO_FOLDER_COLLAPSE_KEY);

  return (
    <div>
      {roots.map((node) => renderNode(node, 0))}
      {noFolder.length > 0 && (
        <div className="mb-4">
          <div
            onClick={() => collapsedFolders.toggle(NO_FOLDER_COLLAPSE_KEY)}
            className="flex items-center gap-2 py-1.5 cursor-pointer select-none rounded-[4px]"
          >
            <span className={`text-[9px] text-[var(--ink-3)] transition-transform ${noFolderCollapsed ? '-rotate-90' : ''}`}>▾</span>
            <span className="text-[13px] opacity-40">📄</span>
            <span className="text-[13px] font-semibold italic text-[var(--ink-2)]">{t('noFolderLabel')}</span>
            <span className="font-mono-ui text-[10px] text-[var(--ink-3)]">{noFolder.length}</span>
            <span className="flex-1 h-px bg-[var(--line)]" />
          </div>
          {!noFolderCollapsed && (
            <div className="mt-2">
              <ModelList windowed={noFolder.length > 60} models={noFolder} {...modelListProps} />
            </div>
          )}
        </div>
      )}
    </div>
  );
}
