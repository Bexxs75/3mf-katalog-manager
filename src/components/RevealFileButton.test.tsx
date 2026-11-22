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
it('caps a large selection at five windows and says so', async () => {
  vi.mocked(invoke).mockResolvedValue(undefined);
  render(<LanguageProvider><RevealFileButton fileId="1" fileIds={['1', '2', '3', '4', '5', '6', '7']} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Im Dateimanager anzeigen' }));
  await waitFor(() => expect(invoke).toHaveBeenCalledTimes(5));
  expect(await screen.findByRole('status')).toHaveTextContent('höchstens 5 Fenster');
  expect(screen.getByRole('status')).toHaveTextContent('2 weitere');
});
it('keeps revealing the other models when one fails', async () => {
  vi.mocked(invoke).mockRejectedValueOnce({ message: 'weg', expected: true, code: 'notFound' }).mockResolvedValue(undefined);
  render(<LanguageProvider><RevealFileButton fileId="1" fileIds={['1', '2', '3']} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Im Dateimanager anzeigen' }));
  await waitFor(() => expect(invoke).toHaveBeenCalledTimes(3));
  expect(await screen.findByRole('alert')).toBeInTheDocument();
});
it('closes the menu on success within the cap but keeps it open to show the cap notice', async () => {
  vi.mocked(invoke).mockResolvedValue(undefined);
  const onSuccess = vi.fn();
  const { unmount } = render(<LanguageProvider><RevealFileButton fileId="1" fileIds={['1', '2']} onSuccess={onSuccess} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Im Dateimanager anzeigen' }));
  await waitFor(() => expect(onSuccess).toHaveBeenCalledTimes(1));
  unmount();
  const over = vi.fn();
  render(<LanguageProvider><RevealFileButton fileId="1" fileIds={['1', '2', '3', '4', '5', '6']} onSuccess={over} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Im Dateimanager anzeigen' }));
  await waitFor(() => expect(invoke).toHaveBeenCalledTimes(7));
  expect(await screen.findByRole('status')).toHaveTextContent('höchstens 5 Fenster');
  expect(over).not.toHaveBeenCalled();
});
