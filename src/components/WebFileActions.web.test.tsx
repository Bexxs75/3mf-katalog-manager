import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { WebFileActions, webFileNameParts } from './WebFileActions';
import { LanguageProvider } from '../i18n/LanguageContext';
import { subscribeFileOpsApplied } from '../lib/fileOps';
import { useEffect, useState } from 'react';
import { authenticationFailed, getSession, login } from '../lib/webSession';
let calls: { command: string; args: Record<string, unknown> }[];
let enabled: boolean;
let response: (command: string) => unknown;
beforeEach(async () => {
  localStorage.clear(); calls = []; enabled = true;
  response = command => command === 'list_folders' ? [{ id: '42', name: 'Target', path: '/private/server', parentId: null }] : command === 'delete_files' || command === 'move_files_to_folder' ? [{ id: '7', ok: true }] : null;
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    if (!url.includes('/invoke/')) return Response.json({ authenticated: true, writable: true, fileOps: enabled, csrfToken: 'csrf' });
    const command = url.split('/').pop()!; calls.push({ command, args: JSON.parse(String(options?.body)) });
    const result = response(command); return result instanceof Response ? result : Response.json(result);
  }));
  await login('test');
});
afterEach(() => { cleanup(); authenticationFailed(401); vi.unstubAllGlobals(); });
function mount(props: Partial<React.ComponentProps<typeof WebFileActions>> = {}) {
  return render(<LanguageProvider><WebFileActions fileIds={['7']} name="model.gcode.3mf" {...props} /></LanguageProvider>);
}
async function confirm(label: string) {
  fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: label }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
}
it('hides all actions without capability and makes no requests', async () => {
  enabled = false; await getSession(); mount(); expect(screen.queryByRole('button')).toBeNull(); expect(calls).toEqual([]);
});
it('requires confirmation and sends only a base name, preserving compound extension', async () => {
  mount(); fireEvent.click(screen.getByRole('button', { name: 'Umbenennen' }));
  expect(calls).toEqual([]); expect(screen.getByRole('textbox')).toHaveValue('model');
  fireEvent.change(screen.getByRole('textbox'), { target: { value: 'New' } }); await confirm('Umbenennen');
  expect(calls).toEqual([{ command: 'rename_file', args: { fileId: '7', name: 'New' } }]);
  expect(webFileNameParts('Test.GCODE.3MF')).toEqual({ base: 'Test', extension: '.GCODE.3MF' });
});
it('moves by folder ID with a required existing destination', async () => {
  mount(); fireEvent.click(screen.getByRole('button', { name: 'Verschieben' }));
  await screen.findByRole('option', { name: 'Target (#42)' });
  expect(within(screen.getByRole('dialog')).getByRole('button', { name: 'Verschieben' })).toBeDisabled();
  fireEvent.change(screen.getByRole('combobox'), { target: { value: '42' } }); await confirm('Verschieben');
  expect(calls[calls.length - 1]).toEqual({ command: 'move_files_to_folder', args: { fileIds: ['7'], folderId: '42' } });
  expect(JSON.stringify(calls)).not.toContain('/private');
});
it('trashes only after confirmation', async () => {
  mount(); fireEvent.click(screen.getByRole('button', { name: 'In den Papierkorb' })); expect(calls).toEqual([]);
  await confirm('In den Papierkorb'); expect(calls[0]).toEqual({ command: 'delete_files', args: { fileIds: ['7'] } });
});
it('requires password and confirm true and explains files kept on disk', async () => {
  response = () => ({ kept_on_disk: true }); mount({ trash: true });
  fireEvent.click(screen.getByRole('button', { name: 'Endgültig löschen' }));
  const dialog = within(screen.getByRole('dialog'));
  expect(dialog.getByRole('button', { name: 'Endgültig löschen' })).toBeDisabled();
  fireEvent.change(dialog.getByLabelText('Passwort zur Bestätigung'), { target: { value: 'secret' } });
  fireEvent.click(dialog.getByRole('button', { name: 'Endgültig löschen' }));
  await screen.findByText(/Die Datei bleibt auf dem Datenträger/);
  expect(calls[0]).toEqual({ command: 'delete_file_permanently', args: { fileId: '7', confirm: true, password: 'secret' } });
  expect(screen.queryByDisplayValue('secret')).toBeNull();
});
it('revokes an open confirmation without sending a write', async () => {
  mount({ trash: true }); fireEvent.click(screen.getByRole('button', { name: 'Endgültig löschen' }));
  fireEvent.change(screen.getByLabelText('Passwort zur Bestätigung'), { target: { value: 'secret' } });
  await act(async () => { enabled = false; await getSession(); });
  expect(screen.getByRole('alert')).toHaveTextContent('nicht mehr verfügbar');
  expect(within(screen.getByRole('dialog')).getByRole('button', { name: 'Endgültig löschen' })).toBeDisabled();
  expect(screen.queryByDisplayValue('secret')).toBeNull(); expect(calls).toEqual([]);
});

it('restores only after confirmation', async () => {
  mount({ trash: true }); fireEvent.click(screen.getByRole('button', { name: 'Wiederherstellen' }));
  expect(calls).toEqual([]); await confirm('Wiederherstellen');
  expect(calls[0]).toEqual({ command: 'restore_file', args: { fileId: '7' } });
});
it('names the file in per-file failures and keeps the confirmation open', async () => {
  response = () => [{ id: '7', ok: false, kind: 'cross_device' }]; mount();
  fireEvent.click(screen.getByRole('button', { name: 'In den Papierkorb' }));
  fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'In den Papierkorb' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('model.gcode.3mf: ');
  expect(screen.getByRole('dialog')).toBeInTheDocument();
});
it('loads trash count and bytes before password confirmation and uses the snapshot protocol', async () => {
  response = command => command === 'empty_trash_summary' ? { count: 1, bytes: 456, maxDeletedAt: '2026-10-01T00:00:00Z' }
    : command === 'empty_trash_start' ? { token: 'frozen', remaining: 1 } : { results: [{ id: '7', ok: true }], remaining: 0 };
  mount({ empty: true }); fireEvent.click(screen.getByRole('button', { name: 'Papierkorb leeren' }));
  await screen.findByText('Dateien / Bytes: 1 / 456');
  expect(calls.map(c => c.command)).toEqual(['empty_trash_summary']);
  fireEvent.change(screen.getByLabelText('Passwort zur Bestätigung'), { target: { value: 'secret' } });
  await confirm('Papierkorb leeren');
  expect(calls[1]).toEqual({ command: 'empty_trash_start', args: { expectedCount: 1, maxDeletedAt: '2026-10-01T00:00:00Z', password: 'secret' } });
  expect(calls[2]).toEqual({ command: 'empty_trash_block', args: { token: 'frozen' } });
});

it('opens keyboard-requested trash confirmation without issuing a request', async () => {
  const closed = vi.fn();
  mount({ requestTrashConfirm: true, onConfirmationClosed: closed });
  expect(screen.getByRole('dialog')).toBeInTheDocument(); expect(calls).toEqual([]);
  fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Abbrechen' }));
  expect(closed).toHaveBeenCalledOnce(); expect(calls).toEqual([]);
});
it('refreshes a conflicting trash summary and requires a new password confirmation', async () => {
  let summaries = 0;
  response = command => command === 'empty_trash_summary' ? { count: ++summaries, bytes: 456, maxDeletedAt: '2026-10-01T00:00:00Z' }
    : Response.json({ kind: 'conflict' }, { status: 409 });
  mount({ empty: true }); fireEvent.click(screen.getByRole('button', { name: 'Papierkorb leeren' }));
  await screen.findByText('Dateien / Bytes: 1 / 456');
  fireEvent.change(screen.getByLabelText('Passwort zur Bestätigung'), { target: { value: 'secret' } });
  fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Papierkorb leeren' }));
  await screen.findByText('Dateien / Bytes: 2 / 456');
  expect(screen.getByLabelText('Passwort zur Bestätigung')).toHaveValue('');
  expect(within(screen.getByRole('dialog')).getByRole('button', { name: 'Papierkorb leeren' })).toBeDisabled();
  expect(calls.map(c => c.command)).toEqual(['empty_trash_summary', 'empty_trash_start', 'empty_trash_summary']);
});

it('shows how many companion files the emptied trash takes along and warns about desktop entries', async () => {
  response = command => command === 'empty_trash_summary'
    ? { count: 2, bytes: 456, maxDeletedAt: '2026-10-01T00:00:00Z', companionCount: 3, blockedByCompanions: 1 } : {};
  mount({ empty: true }); fireEvent.click(screen.getByRole('button', { name: 'Papierkorb leeren' }));
  await screen.findByText('Dateien / Bytes: 2 / 456');
  expect(screen.getByText('davon 3 Begleitdateien im Papierkorb')).toBeInTheDocument();
  expect(screen.getByText(/1 Eintrag aus dem Desktop-Papierkorb kann hier nicht endgültig gelöscht werden/)).toBeInTheDocument();
});
it('adds no companion lines for a server without companion counts or with none', async () => {
  response = command => command === 'empty_trash_summary' ? { count: 1, bytes: 456, maxDeletedAt: '2026-10-01T00:00:00Z' } : {};
  mount({ empty: true }); fireEvent.click(screen.getByRole('button', { name: 'Papierkorb leeren' }));
  await screen.findByText('Dateien / Bytes: 1 / 456');
  expect(screen.queryByText(/Begleitdateien/)).toBeNull();
  expect(screen.queryByText(/Desktop-Papierkorb/)).toBeNull();
});

it('keeps the permanent-delete dialog open after a wrong password, shows the error and refocuses an empty field', async () => {
  response = () => Response.json({ kind: 'password_invalid' }, { status: 403 }); mount({ trash: true });
  fireEvent.click(screen.getByRole('button', { name: 'Endgültig löschen' }));
  fireEvent.change(screen.getByLabelText('Passwort zur Bestätigung'), { target: { value: 'wrong' } });
  fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Endgültig löschen' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Das Passwort ist falsch.');
  expect(screen.getByRole('dialog')).toBeInTheDocument();
  const field = screen.getByLabelText('Passwort zur Bestätigung');
  expect(field).toHaveValue('');
  await waitFor(() => expect(field).toHaveFocus());
  fireEvent.change(field, { target: { value: 'right' } });
  expect(within(screen.getByRole('dialog')).getByRole('button', { name: 'Endgültig löschen' })).toBeEnabled();
});
it('keeps the empty-trash dialog open after a wrong password', async () => {
  response = command => command === 'empty_trash_summary' ? { count: 1, bytes: 5, maxDeletedAt: '2026-10-01T00:00:00Z' } : Response.json({ kind: 'password_invalid' }, { status: 403 });
  mount({ empty: true }); fireEvent.click(screen.getByRole('button', { name: 'Papierkorb leeren' }));
  await screen.findByText('Dateien / Bytes: 1 / 5');
  fireEvent.change(screen.getByLabelText('Passwort zur Bestätigung'), { target: { value: 'wrong' } });
  fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Papierkorb leeren' }));
  expect(await screen.findByText('Das Passwort ist falsch.')).toBeInTheDocument();
  expect(screen.getByRole('dialog')).toBeInTheDocument();
  await waitFor(() => expect(screen.getByLabelText('Passwort zur Bestätigung')).toHaveFocus());
});
it('disables destinations the server marks as no move target and says why', async () => {
  response = command => command === 'list_folders' ? [
    { id: '42', name: 'Target', path: '/p', parentId: null, moveTarget: true },
    { id: '43', name: 'Uploads', path: '/u', parentId: null, moveTarget: false },
    { id: '44', name: 'Old', path: '/o', parentId: null },
  ] : null;
  mount(); fireEvent.click(screen.getByRole('button', { name: 'Verschieben' }));
  const blocked = await screen.findByRole('option', { name: /Uploads/ });
  expect(blocked).toBeDisabled();
  expect(blocked).toHaveTextContent('geschützt');
  expect(screen.getByRole('option', { name: 'Target (#42)' })).toBeEnabled();
  expect(screen.getByRole('option', { name: 'Old (#44)' })).toBeEnabled();
  expect(screen.getByText(/nicht als Ziel verfügbar|gelten nicht als Ziel/)).toBeInTheDocument();
});

it('keeps the permanent-delete dialog open when the surrounding panel drops its selection on every applied action', async () => {
  // Mirrors the detail panel: the app clears the selection (and so unmounts this panel) when an action touched the file.
  function Panel() {
    const [selected, setSelected] = useState<string | null>('7');
    useEffect(() => subscribeFileOpsApplied(ids => setSelected(prev => prev && ids.includes(prev) ? null : prev)), []);
    return selected ? <WebFileActions key={selected} fileIds={[selected]} name="model.gcode.3mf" trash /> : null;
  }
  response = () => Response.json({ kind: 'password_invalid' }, { status: 403 });
  render(<LanguageProvider><Panel /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Endgültig löschen' }));
  fireEvent.change(screen.getByLabelText('Passwort zur Bestätigung'), { target: { value: 'wrong' } });
  fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Endgültig löschen' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Das Passwort ist falsch.');
  await new Promise(resolve => setTimeout(resolve, 300));
  expect(screen.getByRole('dialog')).toBeInTheDocument();
});
