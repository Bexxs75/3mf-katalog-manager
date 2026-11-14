import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { WebFileActions } from './WebFileActions';
import { LanguageProvider } from '../i18n/LanguageContext';
import { authenticationFailed, login } from '../lib/webSession';
let calls: string[];
let gate: ((value: Response) => void) | null;
beforeEach(async () => {
  calls = []; gate = null;
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    if (!url.includes('/invoke/')) return Response.json({ authenticated: true, writable: true, fileOps: true, csrfToken: 'csrf' });
    const command = url.split('/').pop()!; const body = JSON.parse(String(options?.body)); calls.push(command + ':' + (body.fileIds?.length ?? ''));
    if (command === 'list_folders' || command === 'empty_trash_summary') return Response.json(command === 'list_folders' ? [] : { count: 1, bytes: 1, maxDeletedAt: null });
    return new Promise<Response>(resolve => { gate = resolve; });
  }));
  await login('test');
});
afterEach(() => { cleanup(); authenticationFailed(401); vi.unstubAllGlobals(); });

// Befund 2
it('keeps browsers and password managers from autofilling the confirmation password', async () => {
  render(<LanguageProvider><WebFileActions fileIds={['7']} name="m.stl" trash /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Endgültig löschen' }));
  const input = screen.getByLabelText('Passwort zur Bestätigung');
  expect(input).toHaveAttribute('autocomplete', 'new-password');
  expect(input).toHaveAttribute('data-1p-ignore'); expect(input).toHaveAttribute('data-lpignore', 'true'); expect(input).toHaveAttribute('data-form-type', 'other');
});

// Befund 4
it('states the total and the hidden share of a bulk selection and lists the first five names as text', () => {
  const names = ['<b>bold</b>.stl', 'b.stl', 'c.stl', 'd.stl', 'e.stl', 'f.stl', 'g.stl'];
  render(<LanguageProvider><WebFileActions fileIds={names.map((_, i) => String(i))} names={names} hiddenCount={3} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'In den Papierkorb' }));
  const dialog = within(screen.getByRole('dialog'));
  expect(dialog.getByText('7 Dateien, davon 3 nicht sichtbar')).toBeInTheDocument();
  expect(dialog.getAllByRole('listitem').map(li => li.textContent)).toEqual(names.slice(0, 5));
  expect(dialog.queryByText('f.stl')).toBeNull(); expect(dialog.getByText('… und 2 weitere')).toBeInTheDocument();
  expect(screen.getByRole('dialog').querySelector('b')).toBeNull();
});
it('omits the hidden part when everything selected is visible', () => {
  render(<LanguageProvider><WebFileActions fileIds={['1', '2']} names={['a', 'b']} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'In den Papierkorb' }));
  expect(within(screen.getByRole('dialog')).getByText('2 Dateien')).toBeInTheDocument();
});

// Befund 1: Abbrechen während der Ausführung
const ids = Array.from({ length: 450 }, (_, i) => String(i + 1));
async function startBatch() {
  render(<LanguageProvider><WebFileActions fileIds={ids} names={ids} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'In den Papierkorb' }));
  await act(async () => { fireEvent.click(within(screen.getByRole('dialog')).getAllByRole('button', { name: 'In den Papierkorb' }).slice(-1)[0]); });
  expect(calls).toEqual(['delete_files:200']);
}
it('lets the cancel button work while running and sends no further chunk', async () => {
  await startBatch();
  const cancel = within(screen.getByRole('dialog')).getByRole('button', { name: 'Abbrechen' });
  expect(cancel).toBeEnabled(); fireEvent.click(cancel);
  expect(screen.getByText('Wird abgebrochen…')).toBeInTheDocument();
  await act(async () => { gate!(Response.json(ids.slice(0, 200).map(id => ({ id, ok: true })))); });
  expect(calls).toEqual(['delete_files:200']);
  expect(await screen.findByText(/Abgebrochen/)).toBeInTheDocument(); expect(screen.getByRole('dialog')).toBeInTheDocument();
});
it('honours Escape while running', async () => {
  await startBatch();
  fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
  expect(screen.getByText('Wird abgebrochen…')).toBeInTheDocument();
  await act(async () => { gate!(Response.json(ids.slice(0, 200).map(id => ({ id, ok: true })))); });
  expect(calls).toEqual(['delete_files:200']);
});
