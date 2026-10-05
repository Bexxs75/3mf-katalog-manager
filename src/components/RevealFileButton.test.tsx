import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { RevealFileButton } from './RevealFileButton';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); localStorage.setItem('3mf-katalog-language', 'de'); });
it('reveals the catalog ID and visibly reports expected launch failures', async () => {
  vi.mocked(invoke).mockRejectedValue({ message: 'Datei nicht gefunden', expected: true, code: 'notFound' });
  render(<LanguageProvider><RevealFileButton fileId="42" /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Im Dateimanager anzeigen' }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('reveal_in_file_manager', { fileId: '42' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Datei nicht gefunden');
  expect(screen.queryByText('Problem melden')).not.toBeInTheDocument();
});
it('locks the action during imports', () => {
  render(<LanguageProvider><ImportLockContext.Provider value={true}><RevealFileButton fileId="42" /></ImportLockContext.Provider></LanguageProvider>);
  expect(screen.getByRole('button', { name: 'Im Dateimanager anzeigen' })).toBeDisabled();
});
