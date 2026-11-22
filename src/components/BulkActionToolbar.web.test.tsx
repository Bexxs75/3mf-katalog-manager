import { chooseSelect } from '../test/select';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { BulkActionToolbar } from './BulkActionToolbar';
import { LanguageProvider } from '../i18n/LanguageContext';
import { authenticationFailed, login } from '../lib/webSession';
import { clickBulkMore } from '../test/bulkBar';

const requests: { command: string; args: unknown }[] = [];
beforeEach(async () => {
  localStorage.clear(); requests.length = 0;
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    if (!url.includes('/invoke/')) return Response.json({ authenticated: true, writable: true, fileOps: true, csrfToken: 'test' });
    const command = url.split('/').pop()!;
    requests.push({ command, args: JSON.parse(String(options?.body)) });
    return Response.json(command === 'list_folders' ? [{ id: 'target', name: 'Target', parentId: null }] : [{ id: 'a', ok: true }, { id: 'b', ok: true }]);
  }));
  await login('test');
});
afterEach(() => { cleanup(); authenticationFailed(401); vi.unstubAllGlobals(); });

it('opens web move from Ablegen, retains the dialog after the menu closes and moves every selected file', async () => {
  const onDone = vi.fn();
  render(<LanguageProvider><BulkActionToolbar selectedCount={2} selectedIds={['a', 'b']} selectedNames={['A.stl', 'B.stl']}
    confirmBulkDelete={false} onConfirmBulkDeleteChange={vi.fn()} onSelectAllVisible={vi.fn()} onClearSelection={onDone}
    onBulkAddToQueue={vi.fn()} onBulkSetPrintStatus={vi.fn()} onBulkDelete={vi.fn()}
    addToCollectionMenuOpen={false} onAddToCollectionMenuOpenChange={vi.fn()} collections={[]} onBulkAddToCollection={vi.fn()}
    activeCollection={null} onBulkRemoveFromCollection={vi.fn()} addTagMenuOpen={false} onAddTagMenuOpenChange={vi.fn()}
    allTags={[]} tagsInAllSelection={[]} tagDraft="" onTagDraftChange={vi.fn()} onSubmitBulkAddTag={vi.fn()}
    removeTagMenuOpen={false} onRemoveTagMenuOpenChange={vi.fn()} tagsInSelection={[]} onBulkRemoveTag={vi.fn()} />
  </LanguageProvider>);
  expect(within(screen.getByRole('toolbar')).getAllByRole('button').map(button => button.textContent)).toEqual([
    'Alle auswählen', 'Auswahl aufheben', 'Markieren', 'Ablegen', 'Bündel', 'In den Papierkorb',
  ]);
  clickBulkMore('Verschieben nach …');
  expect(screen.queryByRole('menu')).toBeNull();
  await waitFor(() => expect(screen.getByRole('combobox')).toBeEnabled());
  fireEvent.click(screen.getByRole('combobox'));
  expect(screen.getByRole('option', { name: 'Target (#target)' })).toBeInTheDocument();
  fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Abbrechen' }));
  expect(screen.getByRole('button', { name: 'Ablegen' })).toHaveFocus();
  clickBulkMore('Verschieben nach …');
  await waitFor(() => expect(screen.getByRole('combobox')).toBeEnabled());
  fireEvent.click(screen.getByRole('combobox'));
  expect(screen.getByRole('option', { name: 'Target (#target)' })).toBeInTheDocument();
  chooseSelect(screen.getByRole('combobox'), 'target');
  await act(async () => { fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Verschieben' })); });
  await waitFor(() => expect(onDone).toHaveBeenCalledOnce());
  expect(requests.filter(request => request.command === 'move_files_to_folder')).toEqual([
    { command: 'move_files_to_folder', args: { fileIds: ['a', 'b'], folderId: 'target' } },
  ]);
});
