import { chooseSelect } from '../test/select';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { createRef } from 'react';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { FolderCatalogMenu } from './FolderCatalogMenu';
import { LanguageProvider } from '../i18n/LanguageContext';
import { authenticationFailed, getSession } from '../lib/webSession';
import { makeFolder } from '../test/factories';
let enabled: boolean;
let protectedFolder = false;
let summaryModels = 3;
let calls: { command: string; args: unknown }[];
beforeEach(async () => {
  enabled = true; protectedFolder = false; summaryModels = 3; calls = []; localStorage.setItem('3mf-katalog-language', 'en');
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    if (url === '/api/session') return Response.json({ authenticated: true, writable: true, fileOps: enabled, csrfToken: 'token' });
    const command = url.split('/').pop()!;
    calls.push({ command, args: JSON.parse(String(options?.body)) });
    if (command === 'folder_trash_summary') return Response.json({ name: 'source', subfolderCount: 2, modelCount: 3, otherFileCount: 1, otherFileCountTruncated: false, isCatalogRootOrAncestor: protectedFolder });
    if (command === 'trash_folder_start') return Response.json({ token: 'tok', remaining: 3 });
    if (command === 'trash_folder_block') return Response.json({ results: [{ id: '1', ok: true }, { id: '2', ok: true }, { id: '3', ok: true }], remaining: 0, removedFolderCount: 3 });
    return Response.json(command === 'folder_removal_summary' ? { name: 'source', subfolderCount: 2, modelCount: summaryModels } : null);
  }));
  await getSession();
});
afterEach(() => { cleanup(); authenticationFailed(401); vi.unstubAllGlobals(); });
function setup(onTrashed = vi.fn()) {
  return render(<LanguageProvider><FolderCatalogMenu folderId="2" name="source" x={0} y={0} onClose={vi.fn()} onRemoved={vi.fn()} onTrashed={onTrashed} returnFocus={createRef()} folders={[makeFolder({ id: '2' }), makeFolder({ id: '3', name: 'target' })]} /></LanguageProvider>);
}
it('has no action or requests without capability and removes an open dialog on revocation', async () => {
  enabled = false; await getSession(); const view = setup();
  expect(screen.queryByRole('menu')).toBeNull(); expect(calls).toEqual([]); view.unmount();
  enabled = true; await getSession(); setup(); fireEvent.click(screen.getByRole('menuitem', { name: 'Rename folder' }));
  expect(screen.getByRole('dialog')).toBeTruthy();
  await act(async () => { authenticationFailed(401); });
  expect(screen.queryByRole('dialog')).toBeNull(); expect(calls).toEqual([]);
});
it.each([['Create subfolder', 'create_folder', { parentId: '2', name: 'new' }], ['Rename folder', 'rename_folder', { folderId: '2', name: 'new' }]] as const)('confirms %s before mutation', async (label, command, args) => {
  setup(); fireEvent.click(screen.getByRole('menuitem', { name: label }));
  fireEvent.change(screen.getByLabelText('Folder name'), { target: { value: 'new' } });
  expect(calls).toEqual([]); fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
  await waitFor(() => expect(calls).toEqual([{ command, args }]));
});
it.each(['mouse', 'keyboard'] as const)('moves only into a selected folder with a clear contents hint (%s)', async mode => {
  setup(); fireEvent.click(screen.getByRole('menuitem', { name: 'Move folder' }));
  expect(screen.getByText(/uncatalogued files/)).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Confirm' })).toBeDisabled();
  chooseSelect(screen.getByLabelText('Destination folder'), '3', mode);
  expect(calls).toEqual([]); fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
  await waitFor(() => expect(calls).toEqual([{ command: 'move_folder', args: { folderId: '2', newParentId: '3' } }]));
});
it('loads removal counts and asks for the password before the removal of a folder with entries', async () => {
  setup(); fireEvent.click(screen.getByRole('menuitem', { name: /Remove from catalog/i }));
  await waitFor(() => expect(screen.getByLabelText(/Password to confirm/i)).toBeTruthy());
  expect(calls).toEqual([{ command: 'folder_removal_summary', args: { folderId: '2' } }]);
  // Without the password nothing can be confirmed.
  expect(screen.getByRole('button', { name: 'Confirm' })).toBeDisabled();
  fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
  expect(calls).toHaveLength(1);
  fireEvent.change(screen.getByLabelText(/Password to confirm/i), { target: { value: 'secret' } });
  fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
  await waitFor(() => expect(calls[1]).toEqual({ command: 'remove_folder_from_catalog', args: { folderId: '2', password: 'secret' } }));
});
it('removes a folder without entries without a password', async () => {
  summaryModels = 0; setup(); fireEvent.click(screen.getByRole('menuitem', { name: /Remove from catalog/i }));
  await waitFor(() => expect(screen.getByRole('button', { name: 'Confirm' })).not.toBeDisabled());
  expect(screen.queryByLabelText(/Password to confirm/i)).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
  await waitFor(() => expect(calls[1]).toEqual({ command: 'remove_folder_from_catalog', args: { folderId: '2' } }));
});
it('offers moving the folder to the trash, shows the counts and the web note, and runs start and block', async () => {
  const onTrashed = vi.fn(); setup(onTrashed);
  fireEvent.click(screen.getByRole('menuitem', { name: 'Move to trash …' }));
  const dialog = await screen.findByRole('dialog');
  await waitFor(() => expect(screen.getByRole('button', { name: 'Move to trash' })).not.toBeDisabled());
  expect(dialog.textContent).toMatch(/3 models/); expect(dialog.textContent).toMatch(/stay on the server/);
  expect(calls).toEqual([{ command: 'folder_trash_summary', args: { folderId: '2' } }]);
  fireEvent.click(screen.getByRole('button', { name: 'Move to trash' }));
  await waitFor(() => expect(onTrashed).toHaveBeenCalledWith({ trashedModelCount: 3, failed: [], removedFolderCount: 3, keptOnDisk: [] }));
  expect(calls.slice(1)).toEqual([{ command: 'trash_folder_start', args: { folderId: '2' } }, { command: 'trash_folder_block', args: { token: 'tok' } }]);
});
it('refuses the catalog base folder in the trash dialog without a confirm button', async () => {
  protectedFolder = true; setup();
  fireEvent.click(screen.getByRole('menuitem', { name: 'Move to trash …' }));
  await screen.findByText(/cannot be moved to the trash/);
  expect(screen.queryByRole('button', { name: 'Move to trash' })).toBeNull();
  expect(calls.map(c => c.command)).toEqual(['folder_trash_summary']);
});
