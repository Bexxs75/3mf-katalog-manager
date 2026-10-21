import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { MoveTargetDialog } from './MoveTargetDialog';
import { makeFolder } from '../test/factories';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

const folders = [
  makeFolder({ id: 'root', name: 'Katalog', path: '/k', count: 5 }),
  makeFolder({ id: 'c1', name: 'Druckerteile', path: '/k/Druckerteile', parentId: 'root', count: 2 }),
  makeFolder({ id: 'c2', name: 'Deko', path: '/k/Deko', parentId: 'root', count: 3 }),
  makeFolder({ id: 'gone', name: 'Weg', path: '/k/Weg', parentId: 'root', count: 0, missing: true }),
];

function setup(count = 3) {
  const onCancel = vi.fn();
  const onMove = vi.fn();
  render(<LanguageProvider><MoveTargetDialog count={count} folders={folders} onCancel={onCancel} onMove={onMove} /></LanguageProvider>);
  return { onCancel, onMove, tree: screen.getByRole('tree', { name: 'Zielordner' }), confirm: screen.getByRole('button', { name: 'Verschieben' }) };
}

describe('MoveTargetDialog', () => {
  it('is a labelled modal with the count in the title, the tree focused and Verschieben disabled until a folder is chosen', () => {
    const { tree, confirm } = setup();
    expect(screen.getByRole('dialog', { name: '3 Modelle verschieben nach …' })).toHaveAttribute('aria-modal', 'true');
    expect(tree).toHaveFocus();
    expect(confirm).toBeDisabled();
    expect(within(tree).getAllByRole('treeitem').map((item) => item.getAttribute('aria-level'))).toEqual(['1', '2', '2', '2']);
  });

  it('uses the singular title for one model', () => {
    setup(1);
    expect(screen.getByRole('dialog', { name: '1 Modell verschieben nach …' })).toBeVisible();
  });

  it('chooses with the arrow keys, skips folders that are missing on disk and moves with Enter', () => {
    const { tree, onMove, confirm } = setup();
    fireEvent.keyDown(tree, { key: 'ArrowDown' });
    expect(screen.getByRole('treeitem', { name: /Katalog/ })).toHaveAttribute('aria-selected', 'true');
    fireEvent.keyDown(tree, { key: 'ArrowDown' });
    fireEvent.keyDown(tree, { key: 'ArrowDown' });
    expect(screen.getByRole('treeitem', { name: /Deko/ })).toHaveAttribute('aria-selected', 'true');
    fireEvent.keyDown(tree, { key: 'ArrowDown' });
    expect(screen.getByRole('treeitem', { name: /Deko/ })).toHaveAttribute('aria-selected', 'true');
    expect(tree.getAttribute('aria-activedescendant')).toBe('move-target-c2');
    expect(confirm).toBeEnabled();
    fireEvent.keyDown(tree, { key: 'Enter' });
    expect(onMove).toHaveBeenCalledWith('c2');
  });

  it('Enter without a chosen folder does nothing', () => {
    const { tree, onMove } = setup();
    fireEvent.keyDown(tree, { key: 'Enter' });
    expect(onMove).not.toHaveBeenCalled();
  });

  it('Escape and Abbrechen cancel without moving anything', () => {
    const { tree, onCancel, onMove } = setup();
    fireEvent.keyDown(tree, { key: 'ArrowDown' });
    fireEvent.keyDown(tree, { key: 'Escape' });
    expect(onCancel).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole('button', { name: 'Abbrechen' }));
    expect(onCancel).toHaveBeenCalledTimes(2);
    expect(onMove).not.toHaveBeenCalled();
  });

  it('moves into the clicked folder via the Verschieben button', () => {
    const { onMove, confirm } = setup();
    fireEvent.click(screen.getByRole('treeitem', { name: /Druckerteile/ }));
    fireEvent.click(confirm);
    expect(onMove).toHaveBeenCalledWith('c1');
  });

  it('cannot pick a folder that is missing on disk', () => {
    const { onMove, confirm } = setup();
    fireEvent.click(screen.getByRole('treeitem', { name: /Weg/ }));
    expect(confirm).toBeDisabled();
    expect(onMove).not.toHaveBeenCalled();
  });
});
