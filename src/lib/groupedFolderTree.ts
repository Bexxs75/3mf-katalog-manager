import type { Folder, ModelFile } from '../types';

export interface GroupedFolderNode {
  folder: Folder;
  files: ModelFile[];
  children: GroupedFolderNode[];
  totalCount: number;
}

function countAll(node: GroupedFolderNode): number {
  return node.files.length + node.children.reduce((sum, c) => sum + countAll(c), 0);
}

export function buildGroupedFolderTree(
  folders: Folder[],
  models: ModelFile[],
): { roots: GroupedFolderNode[]; noFolder: ModelFile[] } {
  const folderIds = new Set(folders.map((f) => f.id));
  const nodesById = new Map<string, GroupedFolderNode>(
    folders.map((f) => [f.id, { folder: f, files: [], children: [], totalCount: 0 }]),
  );

  const noFolder: ModelFile[] = [];
  for (const m of models) {
    const node = m.folderId && folderIds.has(m.folderId) ? nodesById.get(m.folderId) : undefined;
    if (node) {
      node.files.push(m);
    } else {
      noFolder.push(m);
    }
  }

  const roots: GroupedFolderNode[] = [];
  for (const folder of folders) {
    const node = nodesById.get(folder.id)!;
    if (folder.parentId && nodesById.has(folder.parentId)) {
      nodesById.get(folder.parentId)!.children.push(node);
    } else {
      roots.push(node);
    }
  }

  // Compute totalCount only after the whole tree is built (depends on
  // already filled `children`, recursively from the leaves up).
  const fillCounts = (node: GroupedFolderNode) => {
    node.children.forEach(fillCounts);
    node.totalCount = countAll(node);
  };
  roots.forEach(fillCounts);

  return { roots, noFolder };
}
