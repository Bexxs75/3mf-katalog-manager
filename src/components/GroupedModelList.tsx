import { useEffect, useMemo, useRef, useState } from 'react';
import type { ModelFile, Folder } from '../types';
import { buildGroupedFolderTree, type GroupedFolderNode } from '../lib/groupedFolderTree';
import { ModelList } from './ModelList';
import { useT } from '../i18n/LanguageContext';
import type { useCollapsedFolders } from '../hooks/useCollapsedFolders';

const NO_FOLDER_COLLAPSE_KEY = 'no-folder';

interface Props {
  models: ModelFile[];
  folders: Folder[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onOpenDetail: (id: string) => void;
  onContextMenu: (id: string, x: number, y: number) => void;
  selectedForBulk: Set<string>;
  onToggleBulkSelect: (id: string) => void;
  onDragFileStart?: (id: string) => void;
  draggedFolderId: string | null;
  dragOverFolderId: string | null;
  onDragFolderStart: (id: string) => void;
  onFolderMouseEnter: (id: string) => void;
  onFolderMouseLeave: (id: string) => void;
  collapsedFolders: ReturnType<typeof useCollapsedFolders>;
}

const DRAG_THRESHOLD_PX = 6;

export function GroupedModelList({
  models,
  folders,
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

  // Identisches Schwellenwert-Muster wie FolderTree.tsx's Ordner-Drag (siehe
  // dort) - hier auf die Ordner-Kopfzeilen der gruppierten Ansicht
  // angewendet, damit Ordner auch von hier aus verschoben werden koennen,
  // nicht nur aus der Sidebar.
  const [folderDragCandidateId, setFolderDragCandidateId] = useState<string | null>(null);
  const folderDragStartPos = useRef<{ x: number; y: number } | null>(null);

  useEffect(() => {
    if (!folderDragCandidateId) return;
    const handleMouseMove = (e: MouseEvent) => {
      if (!folderDragStartPos.current) return;
      const dx = e.clientX - folderDragStartPos.current.x;
      const dy = e.clientY - folderDragStartPos.current.y;
      if (Math.hypot(dx, dy) >= DRAG_THRESHOLD_PX) {
        onDragFolderStart(folderDragCandidateId);
        setFolderDragCandidateId(null);
        folderDragStartPos.current = null;
      }
    };
    const handleMouseUp = () => {
      setFolderDragCandidateId(null);
      folderDragStartPos.current = null;
    };
    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [folderDragCandidateId, onDragFolderStart]);

  const handleFolderMouseDown = (e: { clientX: number; clientY: number }, id: string) => {
    folderDragStartPos.current = { x: e.clientX, y: e.clientY };
    setFolderDragCandidateId(id);
  };

  function renderNode(node: GroupedFolderNode, depth: number) {
    // totalCount ist rekursiv (schliesst alle Nachfahren ein) - bei 0 kann
    // dieser Ordner und sein gesamter Unterbaum unter der aktiven Filterung
    // keine Datei enthalten, ein return null hier ist also sicher (Review-
    // Fund I-2: verhindert eine Wand leerer Kopfzeilen bei aktivem Filter).
    if (node.totalCount === 0) return null;
    const isCollapsed = collapsedFolders.isCollapsed(node.folder.id);
    const isDraggedOver = dragOverFolderId === node.folder.id;
    const isBeingDragged = draggedFolderId === node.folder.id;
    return (
      <div key={node.folder.id} className={depth > 0 ? 'ml-3 pl-4 border-l border-[var(--line-strong)] mt-2.5' : 'mb-4'}>
        <div
          onMouseDown={(e) => handleFolderMouseDown(e, node.folder.id)}
          onMouseEnter={() => onFolderMouseEnter(node.folder.id)}
          onMouseLeave={() => onFolderMouseLeave(node.folder.id)}
          onClick={() => collapsedFolders.toggle(node.folder.id)}
          className={`flex items-center gap-2 py-1.5 cursor-pointer select-none rounded-[4px] ${
            isDraggedOver ? 'bg-[var(--accent-soft)] border border-[var(--accent)]' : ''
          } ${isBeingDragged ? 'opacity-40' : ''}`}
        >
          <span className={`text-[9px] text-[var(--ink-3)] transition-transform ${isCollapsed ? '-rotate-90' : ''}`}>▾</span>
          <span className="text-[13px]">📁</span>
          <span className="text-[13px] font-semibold">{node.folder.name}</span>
          <span className="font-mono-ui text-[10px] text-[var(--ink-3)]">{node.totalCount}</span>
          <span className="flex-1 h-px bg-[var(--line)]" />
        </div>
        {!isCollapsed && (
          <div className="mt-2">
            {node.children.map((child) => renderNode(child, depth + 1))}
            {node.files.length > 0 && <ModelList models={node.files} {...modelListProps} />}
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
              <ModelList models={noFolder} {...modelListProps} />
            </div>
          )}
        </div>
      )}
    </div>
  );
}
