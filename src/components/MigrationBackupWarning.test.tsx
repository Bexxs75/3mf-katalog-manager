import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { MigrationBackupWarning } from './MigrationBackupWarning';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { localStorage.setItem('3mf-katalog-language', 'de'); vi.mocked(invoke).mockReset(); });
it('shows a persistent, dismissible warning after backup failure', async () => {
  vi.mocked(invoke).mockResolvedValue(true);
  render(<LanguageProvider><MigrationBackupWarning /></LanguageProvider>);
  expect(await screen.findByRole('alert')).toHaveTextContent('Sicherung');
  expect(invoke).toHaveBeenCalledWith('get_migration_backup_warning');
  fireEvent.click(screen.getByRole('button'));
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});
it('does not warn after a successful or unnecessary backup', async () => {
  vi.mocked(invoke).mockResolvedValue(false);
  render(<LanguageProvider><MigrationBackupWarning /></LanguageProvider>);
  await waitFor(() => expect(invoke).toHaveBeenCalled());
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});
