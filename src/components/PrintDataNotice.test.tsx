import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PrintDataNotice } from './PrintDataNotice';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { de } from '../i18n/de';

const session = vi.hoisted(() => ({ canEdit: true }));
vi.mock('../lib/webSession', async importOriginal => ({ ...(await importOriginal<typeof import('../lib/webSession')>()), useCanEditMetadata: () => session.canEdit }));

beforeEach(() => { localStorage.setItem('3mf-katalog-language', 'de'); session.canEdit = true; });

const show = (props: Partial<React.ComponentProps<typeof PrintDataNotice>> = {}) => render(
  <LanguageProvider><PrintDataNotice hidden={4} shownUnknown={0} includeUnknown={false} onIncludeUnknownChange={vi.fn()} {...props} /></LanguageProvider>,
);

describe('PrintDataNotice unread line', () => {
  it('says how many of the held-back files were never read and opens the place to read them', () => {
    const onReread = vi.fn();
    show({ unread: 3, onReread });
    const notice = screen.getByRole('status');
    expect(notice).toHaveTextContent('4 Dateien ohne Druckdaten ausgeblendet');
    expect(notice).toHaveTextContent('Davon sind 3 noch nicht gelesen.');
    fireEvent.click(screen.getByRole('button', { name: de.pfUnreadReread }));
    expect(onReread).toHaveBeenCalledTimes(1);
  });

  it('uses the singular for one file', () => {
    show({ unread: 1, onReread: vi.fn() });
    expect(screen.getByRole('status')).toHaveTextContent('Davon ist 1 noch nicht gelesen.');
  });

  it('is not there when no held-back file is unread', () => {
    show({ unread: 0, onReread: vi.fn() });
    expect(screen.queryByText(/noch nicht gelesen/)).toBeNull();
    expect(screen.queryByRole('button', { name: de.pfUnreadReread })).toBeNull();
  });

  it('is not there while the files are shown anyway (the cards mark them instead)', () => {
    show({ includeUnknown: true, shownUnknown: 4, unread: 3, onReread: vi.fn() });
    expect(screen.queryByText(/noch nicht gelesen/)).toBeNull();
  });

  it.each([
    ['no place to open', () => show({ unread: 2 })],
    ['no right to change the catalog', () => { session.canEdit = false; return show({ unread: 2, onReread: vi.fn() }); }],
  ])('names the way instead of offering a button: %s', (_name, render) => {
    render();
    expect(screen.queryByRole('button', { name: de.pfUnreadReread })).toBeNull();
    expect(screen.getByText(de.pfUnreadWhere)).toBeVisible();
    expect(screen.getByText('Davon sind 2 noch nicht gelesen.')).toBeVisible();
  });
});
