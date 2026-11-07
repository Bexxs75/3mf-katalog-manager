import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';
import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProviderWithDiagnostics } from '../test/renderWithDiagnostics';
import { DiskSyncSettings } from './DiskSyncSettings';
import { de } from '../i18n/de';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
let saved: { sync_on_start: boolean; sync_auto_add: boolean };
let copies: { path: string; relativePath: string; copyOfName: string | null }[];
let extraCopies: number;
beforeEach(() => {
  copies = [];
  extraCopies = 0;
  localStorage.setItem('3mf-katalog-language', 'de');
  saved = { sync_on_start: false, sync_auto_add: false };
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'get_sync_on_start') return saved.sync_on_start;
    if (cmd === 'get_sync_auto_add') return saved.sync_auto_add;
    if (cmd === 'set_sync_on_start') saved.sync_on_start = (args as { enabled: boolean }).enabled;
    if (cmd === 'set_sync_auto_add') saved.sync_auto_add = (args as { enabled: boolean }).enabled;
    if (cmd === 'count_known_copies') return copies.length;
    if (cmd === 'list_known_copies') return { total: copies.length + extraCopies, items: copies };
    if (cmd === 'reset_known_copies') { const n = copies.length; copies = []; return n; }
  });
});
const setup = () => render(<LanguageProviderWithDiagnostics><DiskSyncSettings /></LanguageProviderWithDiagnostics>);
const onStart = () => screen.getByRole('switch', { name: de.syncOnStartLabel });
const autoAdd = () => screen.getByRole('switch', { name: de.syncAutoAddLabel });

it('starts with both off and the second switch unavailable until the first is on', async () => {
  setup();
  await waitFor(() => expect(onStart()).toBeEnabled());
  expect(onStart()).toHaveAttribute('aria-checked', 'false');
  expect(autoAdd()).toHaveAttribute('aria-checked', 'false');
  expect(autoAdd()).toBeDisabled();
  fireEvent.click(onStart());
  await waitFor(() => expect(autoAdd()).toBeEnabled());
  expect(invoke).toHaveBeenCalledWith('set_sync_on_start', { enabled: true });
  fireEvent.click(autoAdd());
  await waitFor(() => expect(autoAdd()).toHaveAttribute('aria-checked', 'true'));
  expect(invoke).toHaveBeenCalledWith('set_sync_auto_add', { enabled: true });
});

it('turning the first switch off also clears and disables the automatic take-over', async () => {
  saved = { sync_on_start: true, sync_auto_add: true };
  setup();
  await waitFor(() => expect(onStart()).toHaveAttribute('aria-checked', 'true'));
  expect(autoAdd()).toHaveAttribute('aria-checked', 'true');
  fireEvent.click(onStart());
  await waitFor(() => expect(onStart()).toHaveAttribute('aria-checked', 'false'));
  expect(autoAdd()).toHaveAttribute('aria-checked', 'false');
  expect(autoAdd()).toBeDisabled();
  expect(saved).toEqual({ sync_on_start: false, sync_auto_add: false });
});

it('describes both options and warns about the limits for assistive technology', async () => {
  setup();
  await waitFor(() => expect(onStart()).toBeEnabled());
  const text = (box: HTMLElement) => box.getAttribute('aria-describedby')!.split(' ').map(id => document.getElementById(id)!.textContent).join(' ');
  expect(text(onStart())).toBe(de.syncOnStartDescription);
  expect(text(autoAdd())).toBe(`${de.syncAutoAddDescription} ${de.syncAutoAddWarning}`);
  expect(screen.getByText(de.syncAutoAddWarning)).toHaveTextContent('keine Katalogeinträge automatisch entfernt');
});

it('keeps the saved state and shows the error when writing fails', async () => {
  vi.mocked(invoke).mockImplementation(async cmd => {
    if (cmd.startsWith('get_')) return false;
    throw new Error('database unavailable');
  });
  setup();
  await waitFor(() => expect(onStart()).toBeEnabled());
  fireEvent.click(onStart());
  expect(await screen.findByRole('alert')).toBeTruthy();
  expect(onStart()).toHaveAttribute('aria-checked', 'false');
});

const rememberedThree = () => [
  { path: '/models/Spiele/Würfelturm - Kopie.3mf', relativePath: 'Spiele/Würfelturm - Kopie.3mf', copyOfName: 'Würfelturm' },
  { path: '/models/Halter-Kopie.3mf', relativePath: 'Halter-Kopie.3mf', copyOfName: 'Halter' },
  { path: '/models/Halter (alt).3mf', relativePath: 'Halter (alt).3mf', copyOfName: null },
];

it('shows the empty state without remembered copies and disables both buttons', async () => {
  setup();
  expect(await screen.findByText(de.kcEmpty)).toBeVisible();
  expect(screen.getByText('0 gemerkte Kopien')).toBeVisible();
  expect(screen.getByRole('button', { name: de.kcShow })).toBeDisabled();
  expect(screen.getByRole('button', { name: de.kcReset })).toBeDisabled();
});

it('keeps the copy detection in a closed disclosure and says at the reset button when the files return', async () => {
  copies = rememberedThree();
  const { container } = setup();
  expect(await screen.findByText(de.kcHint)).toBeVisible();
  const details = container.querySelector('details')!;
  expect(details).not.toHaveAttribute('open');
  expect(screen.getByText(de.kcHowTitle)).toBeVisible();
  fireEvent.click(screen.getByText(de.kcHowTitle));
  expect(details).toHaveAttribute('open');
  expect(screen.getByText(de.kcHowText)).toBeVisible();
  expect(screen.getByRole('button', { name: de.kcReset })).toHaveAccessibleDescription(`3 gemerkte Kopien ${de.kcResetNote}`);
});

it('shows the dependency of the automatic take-over at the switch while the sync at start is off', async () => {
  setup();
  await waitFor(() => expect(onStart()).toBeEnabled());
  expect(autoAdd()).toBeDisabled();
  expect(autoAdd()).toHaveAccessibleDescription(`${de.syncAutoAddDescription} ${de.syncAutoAddWarning}`);
  expect(screen.getByText(de.syncAutoAddWarning)).toBeVisible();
});

it('names the number of remembered copies and lists them with their original on request', async () => {
  copies = rememberedThree();
  setup();
  expect(await screen.findByText('3 gemerkte Kopien')).toBeVisible();
  expect(invoke).not.toHaveBeenCalledWith('list_known_copies');
  const show = screen.getByRole('button', { name: de.kcShow });
  expect(show).toHaveAttribute('aria-expanded', 'false');
  fireEvent.click(show);
  const list = await screen.findByRole('list', { name: de.kcListLabel });
  const items = within(list).getAllByRole('listitem');
  expect(items).toHaveLength(3);
  expect(items[0]).toHaveTextContent('Spiele/Würfelturm - Kopie.3mf');
  expect(items[0]).toHaveTextContent('Kopie von: Würfelturm');
  expect(items[2]).not.toHaveTextContent('Kopie von');
  const hide = screen.getByRole('button', { name: de.kcHide });
  expect(hide).toHaveAttribute('aria-expanded', 'true');
  expect(hide).toHaveAttribute('aria-controls', list.id);
  fireEvent.click(hide);
  expect(screen.queryByRole('list', { name: de.kcListLabel })).toBeNull();
});

it('keeps focus on the button that opens the list', async () => {
  copies = rememberedThree();
  setup();
  const show = await screen.findByRole('button', { name: de.kcShow });
  await waitFor(() => expect(show).toBeEnabled());
  show.focus();
  fireEvent.click(show);
  await screen.findByRole('list', { name: de.kcListLabel });
  expect(screen.getByRole('button', { name: de.kcHide })).toHaveFocus();
});

it('says how many entries the shortened list leaves out', async () => {
  copies = rememberedThree();
  extraCopies = 499;
  setup();
  fireEvent.click(await screen.findByRole('button', { name: de.kcShow }));
  expect(await screen.findByText('… und 499 weitere')).toBeVisible();
});

it('resets all remembered copies, says so and moves focus to the count', async () => {
  copies = rememberedThree();
  setup();
  const reset = await screen.findByRole('button', { name: de.kcReset });
  await waitFor(() => expect(reset).toBeEnabled());
  reset.focus();
  fireEvent.click(reset);
  expect(await screen.findByText(de.kcResetDone)).toHaveAttribute('role', 'status');
  expect(invoke).toHaveBeenCalledWith('reset_known_copies');
  expect(screen.getByText('0 gemerkte Kopien')).toBeVisible();
  expect(screen.getByRole('button', { name: de.kcReset })).toBeDisabled();
  await waitFor(() => expect(screen.getByText('0 gemerkte Kopien')).toHaveFocus());
});

it('shows the error and keeps the count when resetting fails', async () => {
  copies = rememberedThree();
  setup();
  const reset = await screen.findByRole('button', { name: de.kcReset });
  await waitFor(() => expect(reset).toBeEnabled());
  const impl = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'reset_known_copies') throw { message: 'Datenbank gesperrt', expected: true };
    return impl(cmd, args);
  });
  fireEvent.click(reset);
  expect(await screen.findByRole('alert')).toHaveTextContent('Datenbank gesperrt');
  expect(screen.getByText('3 gemerkte Kopien')).toBeVisible();
});


it.each([['de', de], ['en', en], ['es', es], ['fr', fr]] as const)('keeps the full re-offer action operable in %s', async (language, texts) => {
  localStorage.setItem('3mf-katalog-language', language);
  vi.mocked(invoke).mockImplementation(async command => command === 'count_known_copies' ? 12 : command === 'list_known_copies' ? { total: 12, items: [] } : false);
  render(<LanguageProviderWithDiagnostics><DiskSyncSettings /></LanguageProviderWithDiagnostics>);
  const reset = await screen.findByRole('button', { name: texts.kcReset });
  await waitFor(() => expect(reset).toBeEnabled());
  expect(reset.parentElement).toHaveClass('flex-wrap', 'min-w-0');
  expect(reset).not.toHaveClass('shrink-0');
  fireEvent.click(reset);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('reset_known_copies'));
  expect(await screen.findByRole('status')).toHaveTextContent(texts.kcResetDone);
});

it('keeps the remembered-copy count together when the actions wrap', async () => {
  render(<LanguageProviderWithDiagnostics><DiskSyncSettings /></LanguageProviderWithDiagnostics>);
  await waitFor(() => expect(document.getElementById('known-copies-count')).toHaveClass('whitespace-nowrap', 'flex-auto'));
});
