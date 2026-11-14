import { describe, expect, it, vi } from 'vitest';
import { dropTargetFor, resolveExternalDropTarget } from './externalDrop';
import { isCatalogRootFolder } from './folderTree';
import { makeFolder } from '../test/factories';

const root = makeFolder({ id: 'r', name: 'Katalog', path: '/data/Katalog' });
const sub = makeFolder({ id: 's', name: 'Halter', path: '/data/Katalog/Halter', parentId: 'r' });
const gone = makeFolder({ id: 'g', name: 'Alt', path: '/data/Katalog/Alt', parentId: 'r', missing: true });
const folders = [root, sub, gone];

/** A document with one row per folder; `elementFromPoint` answers with the row `hit`. */
function docWith(hit: string | null, modal = false) {
  const row = hit ? { closest: (sel: string) => (sel === '[data-folder-id]' ? { getAttribute: () => hit } : null) } : null;
  return {
    querySelector: vi.fn(() => (modal ? {} : null)),
    elementFromPoint: vi.fn(() => row),
  } as unknown as Document;
}

describe('dropTargetFor', () => {
  it('accepts a subfolder, refuses a missing folder and the catalog folder itself', () => {
    expect(dropTargetFor(sub, '/data/Katalog').kind).toBe('folder');
    expect(dropTargetFor(gone, '/data/Katalog')).toMatchObject({ kind: 'invalid', reason: 'missing' });
    expect(dropTargetFor(root, '/data/Katalog')).toMatchObject({ kind: 'invalid', reason: 'catalogRoot' });
  });
  it('treats every folder as a target when no catalog folder is set', () => {
    expect(dropTargetFor(root, null).kind).toBe('folder');
  });
});

describe('isCatalogRootFolder', () => {
  it('matches only the catalog folder, not its parent or children', () => {
    expect(isCatalogRootFolder('/data/Katalog', '/data/Katalog')).toBe(true);
    expect(isCatalogRootFolder('/data/Katalog/', '/data/Katalog')).toBe(true);
    expect(isCatalogRootFolder('/data', '/data/Katalog')).toBe(false);
    expect(isCatalogRootFolder('/data/Katalog/Halter', '/data/Katalog')).toBe(false);
    expect(isCatalogRootFolder('C:\\Data\\Katalog', 'c:/data/katalog')).toBe(true);
  });
});

describe('resolveExternalDropTarget', () => {
  it('finds the folder row under the pointer', () => {
    const doc = docWith('s');
    expect(resolveExternalDropTarget({ x: 10, y: 20 }, 1, false, folders, '/data/Katalog', doc)).toMatchObject({ kind: 'folder', folder: { id: 's' } });
    expect(doc.elementFromPoint).toHaveBeenCalledWith(10, 20);
  });
  it('divides by the device pixel ratio on Windows only', () => {
    const win = docWith('s');
    resolveExternalDropTarget({ x: 300, y: 150 }, 1.5, true, folders, null, win);
    expect(win.elementFromPoint).toHaveBeenCalledWith(200, 100);
    const other = docWith('s');
    resolveExternalDropTarget({ x: 300, y: 150 }, 1.5, false, folders, null, other);
    expect(other.elementFromPoint).toHaveBeenCalledWith(300, 150);
  });
  it('reports no target beside the tree, for unknown ids and while a modal dialog is open', () => {
    expect(resolveExternalDropTarget({ x: 0, y: 0 }, 1, false, folders, null, docWith(null)).kind).toBe('none');
    expect(resolveExternalDropTarget({ x: 0, y: 0 }, 1, false, folders, null, docWith('unknown')).kind).toBe('none');
    expect(resolveExternalDropTarget({ x: 0, y: 0 }, 1, false, folders, null, docWith('s', true)).kind).toBe('none');
  });
  it('marks missing and catalog folders as invalid', () => {
    expect(resolveExternalDropTarget({ x: 0, y: 0 }, 1, false, folders, '/data/Katalog', docWith('g'))).toMatchObject({ kind: 'invalid', reason: 'missing' });
    expect(resolveExternalDropTarget({ x: 0, y: 0 }, 1, false, folders, '/data/Katalog', docWith('r'))).toMatchObject({ kind: 'invalid', reason: 'catalogRoot' });
  });
});
