import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProviderWithDiagnostics } from '../test/renderWithDiagnostics';
import { DiskSyncSettings } from './DiskSyncSettings';
import { de } from '../i18n/de';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
let saved: { sync_on_start: boolean; sync_auto_add: boolean };
beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  saved = { sync_on_start: false, sync_auto_add: false };
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'get_sync_on_start') return saved.sync_on_start;
    if (cmd === 'get_sync_auto_add') return saved.sync_auto_add;
    if (cmd === 'set_sync_on_start') saved.sync_on_start = (args as { enabled: boolean }).enabled;
    if (cmd === 'set_sync_auto_add') saved.sync_auto_add = (args as { enabled: boolean }).enabled;
  });
});
const setup = () => render(<LanguageProviderWithDiagnostics><DiskSyncSettings /></LanguageProviderWithDiagnostics>);
const onStart = () => screen.getByRole('checkbox', { name: de.syncOnStartLabel }) as HTMLInputElement;
const autoAdd = () => screen.getByRole('checkbox', { name: de.syncAutoAddLabel }) as HTMLInputElement;

it('starts with both off and the second box greyed out until the first is on', async () => {
  setup();
  await waitFor(() => expect(onStart()).toBeEnabled());
  expect(onStart().checked).toBe(false);
  expect(autoAdd().checked).toBe(false);
  expect(autoAdd()).toBeDisabled();
  fireEvent.click(onStart());
  await waitFor(() => expect(autoAdd()).toBeEnabled());
  expect(invoke).toHaveBeenCalledWith('set_sync_on_start', { enabled: true });
  fireEvent.click(autoAdd());
  await waitFor(() => expect(autoAdd().checked).toBe(true));
  expect(invoke).toHaveBeenCalledWith('set_sync_auto_add', { enabled: true });
});

it('turning the first box off also clears and disables the automatic take-over', async () => {
  saved = { sync_on_start: true, sync_auto_add: true };
  setup();
  await waitFor(() => expect(onStart().checked).toBe(true));
  expect(autoAdd().checked).toBe(true);
  fireEvent.click(onStart());
  await waitFor(() => expect(onStart().checked).toBe(false));
  expect(autoAdd().checked).toBe(false);
  expect(autoAdd()).toBeDisabled();
  expect(saved).toEqual({ sync_on_start: false, sync_auto_add: false });
});

it('describes both options and warns about the limits for assistive technology', async () => {
  setup();
  await waitFor(() => expect(onStart()).toBeEnabled());
  const text = (box: HTMLElement) => box.getAttribute('aria-describedby')!.split(' ').map(id => document.getElementById(id)!.textContent).join(' ');
  expect(text(onStart())).toBe(de.syncOnStartDescription);
  expect(text(autoAdd())).toBe(`${de.syncAutoAddDescription} ${de.syncAutoAddWarning}`);
  expect(screen.getByText(de.syncAutoAddWarning)).toHaveTextContent('nie Dateien aus dem Katalog entfernt');
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
  expect(onStart().checked).toBe(false);
});
