import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor, cleanup, fireEvent } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';

const api = vi.hoisted(() => ({ list: vi.fn(), image: vi.fn(), remove: vi.fn(), restore: vi.fn(), upload: vi.fn() }));
vi.mock('../lib/api/companions', () => ({
  listWebCompanions: api.list,
  companionImageData: api.image,
  companionDownloadUrl: (id: string) => `/api/companions/${id}/download`,
  removeWebCompanion: api.remove,
  restoreWebCompanion: api.restore,
  uploadWebCompanion: api.upload,
  COMPANION_ACCEPT: '.png,.pdf,.txt',
  companionSizeProblem: (file: File) => (file.size === 0 ? 'empty' : file.size > 100 ? 'too_large' : null),
  maxCompanionBytes: () => 100,
  isAllowedCompanionName: (name: string) => /\.(png|jpe?g|gif|webp|pdf|txt|md)$/i.test(name),
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
  api.list.mockReset(); api.image.mockReset(); session.mockReset(); api.remove.mockReset(); api.restore.mockReset(); api.upload.mockReset();
  api.upload.mockResolvedValue({ id: '9' });
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

  describe('adding files', () => {
    const pick = (files: File[]) => {
      const input = document.querySelector('input[type=file]') as HTMLInputElement;
      fireEvent.change(input, { target: { files } });
    };

    it('offers adding only with the file-management right', async () => {
      api.list.mockResolvedValue([pdf]);
      show();
      await screen.findByText('Anleitung.pdf');
      expect(screen.queryByRole('button', { name: /Hinzufügen/ })).toBeNull();
      expect(document.querySelector('input[type=file]')).toBeNull();
      expect(screen.queryByText(/hierher ziehen/)).toBeNull();
    });

    it('uploads the picked files one after the other, then reloads, names the count and focuses the button', async () => {
      manage.allowed = true;
      api.list.mockResolvedValue([]);
      const order: string[] = [];
      api.upload.mockImplementation(async (_id: string, file: File, progress: (n: number) => void) => { progress(40); order.push(file.name); return { id: '9' }; });
      show();
      await screen.findByText('Keine Begleitdateien.');
      expect(screen.getByText(/hierher ziehen/)).toBeTruthy();
      pick([new File(['a'], 'a.png'), new File(['b'], 'b.pdf')]);
      await waitFor(() => expect(order).toEqual(['a.png', 'b.pdf']));
      expect(api.upload).toHaveBeenCalledWith('1', expect.any(File), expect.any(Function));
      expect(await screen.findByText('2 Begleitdateien hinzugefügt.')).toBeTruthy();
      await waitFor(() => expect(document.activeElement).toBe(screen.getByRole('button', { name: /Hinzufügen/ })));
      expect(api.list.mock.calls.length).toBeGreaterThanOrEqual(2);
    });

    it('shows progress while a file goes up and disables the button', async () => {
      manage.allowed = true;
      api.list.mockResolvedValue([]);
      let finish: () => void = () => {};
      api.upload.mockImplementation((_id: string, _f: File, progress: (n: number) => void) => { progress(55); return new Promise((resolve) => { finish = () => resolve({ id: '9' }); }); });
      show();
      await screen.findByText('Keine Begleitdateien.');
      pick([new File(['a'], 'a.png')]);
      expect((await screen.findByRole('status')).textContent).toContain('a.png (1 von 1), 55 %');
      expect(screen.getByRole('button', { name: /Hinzufügen/ })).toBeDisabled();
      finish();
      await screen.findByText('1 Begleitdatei hinzugefügt.');
    });

    it('names each refused file and still uploads the others; a wrong type never goes up', async () => {
      manage.allowed = true;
      api.list.mockResolvedValue([]);
      api.upload.mockImplementation(async (_id: string, file: File) => {
        if (file.name === 'zu-gross.pdf') throw { message: 'Die Datei ist größer als die Grenze', expected: true };
        return { id: '9' };
      });
      show();
      await screen.findByText('Keine Begleitdateien.');
      pick([new File(['x'], 'virus.exe'), new File(['x'], 'zu-gross.pdf'), new File(['x'], 'ok.png')]);
      await waitFor(() => expect(screen.getAllByRole('alert')).toHaveLength(2));
      const alerts = screen.getAllByRole('alert').map((a) => a.textContent ?? '');
      expect(alerts[0]).toContain('virus.exe');
      expect(alerts[0]).toMatch(/nicht erlaubt/);
      expect(alerts[1]).toContain('zu-gross.pdf');
      expect(api.upload).toHaveBeenCalledTimes(2);
      expect(api.upload.mock.calls.map((c) => (c[1] as File).name)).toEqual(['zu-gross.pdf', 'ok.png']);
      expect(await screen.findByText('1 Begleitdatei hinzugefügt.')).toBeTruthy();
    });

    it('names the size and the limit of a file that is too big, and an empty one, without sending them', async () => {
      manage.allowed = true;
      api.list.mockResolvedValue([]);
      show();
      await screen.findByText('Keine Begleitdateien.');
      pick([new File(['x'.repeat(200)], 'gross.pdf'), new File([], 'leer.txt'), new File(['x'], 'ok.png')]);
      await waitFor(() => expect(screen.getAllByRole('alert')).toHaveLength(2));
      const alerts = screen.getAllByRole('alert').map((a) => a.textContent ?? '');
      expect(alerts[0]).toMatch(/gross\.pdf.*200 B.*zu groß.*bis 100 B/s);
      expect(alerts[1]).toMatch(/leer\.txt.*leer/);
      expect(api.upload.mock.calls.map((c) => (c[1] as File).name)).toEqual(['ok.png']);
    });

    it('takes dropped files, and ignores a drag that carries no files', async () => {
      manage.allowed = true;
      api.list.mockResolvedValue([]);
      const { container } = show();
      await screen.findByText('Keine Begleitdateien.');
      const section = container.querySelector('section') as HTMLElement;
      fireEvent.drop(section, { dataTransfer: { types: ['text/plain'], files: [new File(['x'], 'a.png')] } });
      expect(api.upload).not.toHaveBeenCalled();
      fireEvent.drop(section, { dataTransfer: { types: ['Files'], files: [new File(['x'], 'a.png')] } });
      await waitFor(() => expect(api.upload).toHaveBeenCalledTimes(1));
    });

    it('does not start a second upload while one runs', async () => {
      manage.allowed = true;
      api.list.mockResolvedValue([]);
      api.upload.mockReturnValue(new Promise(() => {}));
      show();
      await screen.findByText('Keine Begleitdateien.');
      pick([new File(['x'], 'a.png')]);
      await waitFor(() => expect(api.upload).toHaveBeenCalledTimes(1));
      pick([new File(['x'], 'b.png')]);
      expect(api.upload).toHaveBeenCalledTimes(1);
    });
  });
});
