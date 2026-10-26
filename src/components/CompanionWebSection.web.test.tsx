import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor, cleanup, fireEvent } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';

const api = vi.hoisted(() => ({ list: vi.fn(), image: vi.fn(), remove: vi.fn(), restore: vi.fn() }));
vi.mock('../lib/api/companions', () => ({
  listWebCompanions: api.list,
  companionImageData: api.image,
  companionDownloadUrl: (id: string) => `/api/companions/${id}/download`,
  removeWebCompanion: api.remove,
  restoreWebCompanion: api.restore,
}));
const session = vi.hoisted(() => vi.fn());
const manage = vi.hoisted(() => ({ allowed: false }));
vi.mock('../lib/webSession', () => ({ getSession: session, useCanManageFiles: () => manage.allowed }));

import { CompanionWebSection } from './CompanionWebSection';

const base = { fileId: '1', addedAt: 'now', missing: false, previewable: false };
const pdf = { ...base, id: '2', name: 'Anleitung.pdf', kind: 'pdf', sizeBytes: 2048 };
const png = { ...base, id: '3', name: 'foto.png', kind: 'image', sizeBytes: 10, previewable: true };
const gone = { ...base, id: '4', name: 'weg.txt', kind: 'text', sizeBytes: 5, missing: true };

const show = () => render(<LanguageProvider><CompanionWebSection fileId="1" variant="page" /></LanguageProvider>);

beforeEach(() => {
  api.list.mockReset(); api.image.mockReset(); session.mockReset(); api.remove.mockReset(); api.restore.mockReset();
  manage.allowed = false;
  api.remove.mockResolvedValue(undefined); api.restore.mockResolvedValue(undefined);
  session.mockResolvedValue({ authenticated: true, mustChangePassword: false });
  api.image.mockResolvedValue('data:image/png;base64,AAAA');
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe('CompanionWebSection', () => {
  it('lists name and size with a download button, and offers nothing that changes files', async () => {
    api.list.mockResolvedValue([pdf, gone]);
    show();
    expect(await screen.findByText('Anleitung.pdf')).toBeTruthy();
    expect(screen.getByRole('button', { name: /Anleitung\.pdf herunterladen/ })).toBeTruthy();
    expect(screen.getByText('Datei fehlt')).toBeTruthy();
    expect(screen.queryByRole('button', { name: /weg\.txt herunterladen/ })).toBeNull();
    for (const label of [/Hinzufügen|Add/, /Entfernen|Remove/, /Wiederherstellen|Restore/]) {
      expect(screen.queryByRole('button', { name: label })).toBeNull();
    }
  });

  it('shows the empty state and a load error', async () => {
    api.list.mockResolvedValueOnce([]);
    const { unmount } = show();
    expect(await screen.findByText('Keine Begleitdateien.')).toBeTruthy();
    unmount();
    api.list.mockRejectedValueOnce({ message: 'kaputt', expected: true });
    show();
    expect(await screen.findByRole('alert')).toBeTruthy();
  });

  it('fetches a picture preview only for previewable images', async () => {
    api.list.mockResolvedValue([png, { ...png, id: '5', name: 'gross.png', previewable: false }]);
    show();
    await waitFor(() => expect(api.image).toHaveBeenCalledWith('3'));
    expect(api.image).not.toHaveBeenCalledWith('5');
  });

  it('only shows a preview whose source is a data:image URI', async () => {
    api.list.mockResolvedValue([png]);
    api.image.mockResolvedValue('javascript:alert(1)');
    const { container } = show();
    await waitFor(() => expect(api.image).toHaveBeenCalled());
    await Promise.resolve();
    expect(container.querySelector('img')).toBeNull();
  });

  it('downloads through the same-origin attachment route after checking the session', async () => {
    api.list.mockResolvedValue([pdf]);
    const clicked: string[] = [];
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { clicked.push(this.getAttribute('href') ?? ''); });
    show();
    fireEvent.click(await screen.findByRole('button', { name: /herunterladen/ }));
    await waitFor(() => expect(clicked).toEqual(['/api/companions/2/download']));
  });

  it('does not start a download without a valid session', async () => {
    api.list.mockResolvedValue([pdf]);
    session.mockResolvedValue({ authenticated: false, mustChangePassword: false });
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
    show();
    fireEvent.click(await screen.findByRole('button', { name: /herunterladen/ }));
    await waitFor(() => expect(session).toHaveBeenCalled());
    expect(click).not.toHaveBeenCalled();
  });

  describe('removing and restoring single files', () => {
    const removedPdf = { ...pdf, id: '7', name: 'alt.pdf', deletedAt: '2026-10-01T00:00:00Z', restorable: true };
    const removedAtDesktop = { ...pdf, id: '8', name: 'desktop.pdf', deletedAt: '2026-10-01T00:00:00Z', restorable: false };

    it('offers neither remove nor the removed list without the file-management right', async () => {
      api.list.mockResolvedValue([pdf, removedPdf]);
      show();
      expect(await screen.findByText('Anleitung.pdf')).toBeTruthy();
      expect(screen.queryByRole('button', { name: /entfernen/i })).toBeNull();
      expect(screen.queryByText('alt.pdf')).toBeNull();
      expect(screen.queryByText(/^Entfernt \(/)).toBeNull();
      expect(api.list).toHaveBeenCalledWith('1', false);
    });

    it('removes without asking, offers undo, and undo restores the same file', async () => {
      manage.allowed = true;
      api.list.mockResolvedValueOnce([pdf, png]).mockResolvedValueOnce([png]).mockResolvedValue([pdf, png]);
      show();
      fireEvent.click(await screen.findByRole('button', { name: /„Anleitung\.pdf“ entfernen/ }));
      await waitFor(() => expect(api.remove).toHaveBeenCalledWith('2'));
      expect(await screen.findByText(/wurde in den Papierkorb gelegt/)).toBeTruthy();
      expect(screen.queryByText('Anleitung.pdf')).toBeNull();
      fireEvent.click(screen.getByRole('button', { name: 'Rückgängig' }));
      await waitFor(() => expect(api.restore).toHaveBeenCalledWith('2'));
      expect(await screen.findByText(/wurde wiederhergestellt/)).toBeTruthy();
      expect(screen.queryByRole('button', { name: 'Rückgängig' })).toBeNull();
      expect(api.list).toHaveBeenCalledTimes(3);
    });

    it('moves the focus to the heading after a removal, because the used button is gone', async () => {
      manage.allowed = true;
      api.list.mockResolvedValueOnce([pdf]).mockResolvedValue([]);
      show();
      fireEvent.click(await screen.findByRole('button', { name: /entfernen/ }));
      await waitFor(() => expect(document.activeElement?.tagName).toBe('H3'));
    });

    it('lists removed files with restore, and sends restorable=false ones to the desktop', async () => {
      manage.allowed = true;
      api.list.mockResolvedValue([pdf, removedPdf, removedAtDesktop]);
      show();
      expect(await screen.findByText('Entfernt (2)')).toBeTruthy();
      expect(api.list).toHaveBeenCalledWith('1', true);
      expect(screen.getByText(/am Desktop wiederherstellen/)).toBeTruthy();
      expect(screen.queryByRole('button', { name: /desktop\.pdf wiederherstellen/ })).toBeNull();
      fireEvent.click(screen.getByRole('button', { name: /alt\.pdf wiederherstellen/ }));
      await waitFor(() => expect(api.restore).toHaveBeenCalledWith('7'));
      // Removed files are not part of the live count in the heading.
      expect(screen.getByRole('heading', { level: 3 }).textContent).toBe('Begleitdateien (1)');
    });

    it('shows a refused removal and a refused restore in their own alerts and keeps the list', async () => {
      manage.allowed = true;
      api.list.mockResolvedValue([pdf, removedPdf]);
      api.remove.mockRejectedValueOnce({ message: 'Eine Begleitdatei kann nicht entfernt werden', expected: true });
      api.restore.mockRejectedValueOnce({ message: 'Das Modell hat schon die höchste Zahl', expected: true });
      show();
      fireEvent.click(await screen.findByRole('button', { name: /„Anleitung\.pdf“ entfernen/ }));
      expect((await screen.findByRole('alert')).textContent).toContain('kann nicht entfernt werden');
      fireEvent.click(await screen.findByRole('button', { name: /alt\.pdf wiederherstellen/ }));
      await waitFor(() => expect(screen.getAllByRole('alert').some((a) => a.textContent?.includes('Wiederherstellen nicht möglich'))).toBe(true));
      expect(screen.getByText('Anleitung.pdf')).toBeTruthy();
    });

    it('runs one action at a time', async () => {
      manage.allowed = true;
      let release: () => void = () => {};
      api.remove.mockReturnValue(new Promise<void>((resolve) => { release = resolve; }));
      api.list.mockResolvedValue([pdf, png]);
      show();
      const button = await screen.findByRole('button', { name: /„Anleitung\.pdf“ entfernen/ });
      fireEvent.click(button);
      fireEvent.click(button);
      expect(screen.getByRole('button', { name: /„foto\.png“ entfernen/ })).toBeDisabled();
      fireEvent.click(screen.getByRole('button', { name: /„foto\.png“ entfernen/ }));
      release();
      await waitFor(() => expect(api.remove).toHaveBeenCalledTimes(1));
    });
  });
});
