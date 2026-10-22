import { expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as Provider } from '../test/renderWithDiagnostics';
import { FolderTree } from './FolderTree';
import { useFolderExpansion } from '../hooks/useFolderExpansion';
import { makeFolder } from '../test/factories';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));

function Tree({ count }: { count: number }) {
  return <FolderTree expansion={useFolderExpansion()} folders={[makeFolder({ id: 'f1', name: 'Druckerteile' })]} totalModelCount={5}
    activeFolderId="all" onSelect={() => {}} draggedFileId="m1" draggedFileFolderId="other" draggedFileCount={count} dragOverFolderId="f1" />;
}

it('names the number of models at the folder under the pointer when several are dragged', () => {
  render(<Provider><Tree count={3} /></Provider>);
  expect(screen.getByText('Hier ablegen: 3 Modelle verschieben')).toBeInTheDocument();
});

it('keeps the short label for a single model', () => {
  render(<Provider><Tree count={1} /></Provider>);
  expect(screen.getByText('hierher')).toBeInTheDocument();
});
