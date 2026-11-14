import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { createRef } from 'react';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { FolderCatalogMenu } from './FolderCatalogMenu';
import { LanguageProvider } from '../i18n/LanguageContext';
import { authenticationFailed, getSession } from '../lib/webSession';
import { makeFolder } from '../test/factories';
let enabled: boolean;
let calls: { command: string; args: unknown }[];
beforeEach(async () => {
  enabled = true; calls = []; localStorage.setItem('3mf-katalog-language', 'en');
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    if (url === '/api/session') return Response.json({ authenticated: true, writable: true, fileOps: enabled, csrfToken: 'token' });
    const command = url.split('/').pop()!;
    calls.push({ command, args: JSON.parse(String(options?.body)) });
    return Response.json(command === 'folder_removal_summary' ? { name: 'source', subfolderCount: 2, modelCount: 3 } : null);
  }));
  await getSession();
});
afterEach(() => { cleanup(); authenticationFailed(401); vi.unstubAllGlobals(); });
function setup() {
  return render(<LanguageProvider><FolderCatalogMenu folderId="2" name="source" x={0} y={0} onClose={vi.fn()} onRemoved={vi.fn()} returnFocus={createRef()} folders={[makeFolder({ id: '2' }), makeFolder({ id: '3', name: 'target' })]} /></LanguageProvider>);
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
it('moves only into a selected folder with a clear contents hint', async () => {
  setup(); fireEvent.click(screen.getByRole('menuitem', { name: 'Move folder' }));
  expect(screen.getByText(/uncatalogued files/)).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Confirm' })).toBeDisabled();
  fireEvent.change(screen.getByLabelText('Destination folder'), { target: { value: '3' } });
  expect(calls).toEqual([]); fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
  await waitFor(() => expect(calls).toEqual([{ command: 'move_folder', args: { folderId: '2', newParentId: '3' } }]));
});
it('loads removal counts before explicit removal confirmation', async () => {
  setup(); fireEvent.click(screen.getByRole('menuitem', { name: /Remove from catalog/i }));
  await waitFor(() => expect(screen.getByRole('button', { name: 'Confirm' })).not.toBeDisabled());
  expect(calls).toEqual([{ command: 'folder_removal_summary', args: { folderId: '2' } }]);
  fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
  await waitFor(() => expect(calls[1]).toEqual({ command: 'remove_folder_from_catalog', args: { folderId: '2' } }));
});
