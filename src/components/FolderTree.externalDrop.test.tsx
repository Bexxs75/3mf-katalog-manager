import { expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as Provider } from '../test/renderWithDiagnostics';
import { FolderTree } from './FolderTree';
import { useFolderExpansion } from '../hooks/useFolderExpansion';
import { ExternalDragContext, type ExternalDragState } from '../hooks/useExternalDropTarget';
import { makeFolder } from '../test/factories';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));

const folders = [makeFolder({ id: 'f1', name: 'Halter' }), makeFolder({ id: 'f2', name: 'Alt', missing: true })];
function Tree({ state }: { state: ExternalDragState }) {
  return <Provider><ExternalDragContext.Provider value={state}>
    <FolderTree expansion={useFolderExpansion()} folders={folders} totalModelCount={5} activeFolderId="all" onSelect={() => {}} />
  </ExternalDragContext.Provider></Provider>;
}
const idle: ExternalDragState = { target: { kind: 'none' }, count: 0 };

it('marks every folder row with its id for the drop lookup', () => {
  const { container } = render(<Tree state={idle} />);
  expect([...container.querySelectorAll('[data-folder-id]')].map(e => e.getAttribute('data-folder-id'))).toEqual(['f1', 'f2']);
});
it('shows no drop text and no current marker without a drag', () => {
  const { container } = render(<Tree state={idle} />);
  expect(screen.queryByText('hierher')).not.toBeInTheDocument();
  expect(container.querySelector('[aria-current]')).toBeNull();
});
it('highlights the folder under the pointer with frame, text and name', () => {
  render(<Tree state={{ target: { kind: 'folder', folder: folders[0] }, count: 1 }} />);
  const row = screen.getByRole('button', { name: 'Zeiger über Halter' });
  expect(row).toHaveAttribute('aria-current', 'true');
  expect(row.className).toContain('border-[var(--accent)]');
  expect(screen.getByText('hierher')).toBeInTheDocument();
});
it('names the number of dragged files', () => {
  render(<Tree state={{ target: { kind: 'folder', folder: folders[0] }, count: 3 }} />);
  expect(screen.getByText('Hier ablegen: 3 Dateien verschieben')).toBeInTheDocument();
});
it('says "kein Ziel" for a folder that cannot take the drop', () => {
  render(<Tree state={{ target: { kind: 'invalid', folder: folders[1], reason: 'missing' }, count: 2 }} />);
  expect(screen.getByRole('button', { name: 'Zeiger über Alt – kein gültiges Ziel' })).toBeInTheDocument();
  expect(screen.getByText('kein Ziel')).toBeInTheDocument();
  expect(screen.queryByText(/Hier ablegen/)).not.toBeInTheDocument();
});
