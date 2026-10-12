import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProviderWithDiagnostics } from '../test/renderWithDiagnostics';
import { ImportRenameSetting } from './ImportRenameSetting';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(invoke).mockReset();
});
const setup = () => render(<LanguageProviderWithDiagnostics><ImportRenameSetting /></LanguageProviderWithDiagnostics>);
it('defaults off, saves both switch states and reloads the saved setting', async () => {
  let saved = false;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'get_import_rename_enabled') return saved;
    if (cmd === 'set_import_rename_enabled') saved = (args as { enabled: boolean }).enabled;
  });
  const first = setup();
  const toggle = screen.getByRole('switch', { name: 'Dateien beim Import umbenennen' });
  await waitFor(() => expect(toggle).toBeEnabled());
  expect(toggle).toHaveAttribute('aria-checked', 'false');
  fireEvent.click(toggle);
  await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'true'));
  expect(invoke).toHaveBeenCalledWith('set_import_rename_enabled', { enabled: true });
  first.unmount(); setup();
  const reloaded = screen.getByRole('switch', { name: 'Dateien beim Import umbenennen' });
  await waitFor(() => expect(reloaded).toHaveAttribute('aria-checked', 'true'));
  fireEvent.click(reloaded);
  await waitFor(() => expect(reloaded).toHaveAttribute('aria-checked', 'false'));
  expect(invoke).toHaveBeenCalledWith('set_import_rename_enabled', { enabled: false });
});
it('keeps the saved state when writing fails', async () => {
  vi.mocked(invoke).mockImplementation(async cmd => {
    if (cmd === 'get_import_rename_enabled') return false;
    throw new Error('database unavailable');
  });
  setup();
  const toggle = screen.getByRole('switch');
  await waitFor(() => expect(toggle).toBeEnabled());
  fireEvent.click(toggle);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('set_import_rename_enabled', { enabled: true }));
  await waitFor(() => expect(toggle).toBeEnabled());
  expect(toggle).toHaveAttribute('aria-checked', 'false');
  expect(screen.getByText(/database unavailable/)).toBeVisible();
});
