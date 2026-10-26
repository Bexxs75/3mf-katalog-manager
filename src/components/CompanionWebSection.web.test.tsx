import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor, cleanup, fireEvent } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';

const api = vi.hoisted(() => ({ list: vi.fn(), image: vi.fn() }));
vi.mock('../lib/api/companions', () => ({
  listWebCompanions: api.list,
  companionImageData: api.image,
  companionDownloadUrl: (id: string) => `/api/companions/${id}/download`,
}));
const session = vi.hoisted(() => vi.fn());
vi.mock('../lib/webSession', () => ({ getSession: session }));

import { CompanionWebSection } from './CompanionWebSection';

const base = { fileId: '1', addedAt: 'now', missing: false, previewable: false };
const pdf = { ...base, id: '2', name: 'Anleitung.pdf', kind: 'pdf', sizeBytes: 2048 };
const png = { ...base, id: '3', name: 'foto.png', kind: 'image', sizeBytes: 10, previewable: true };
const gone = { ...base, id: '4', name: 'weg.txt', kind: 'text', sizeBytes: 5, missing: true };

const show = () => render(<LanguageProvider><CompanionWebSection fileId="1" variant="page" /></LanguageProvider>);

beforeEach(() => {
  api.list.mockReset(); api.image.mockReset(); session.mockReset();
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
});
