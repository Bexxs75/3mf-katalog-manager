import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { BlockedAutoTagsSettings } from './BlockedAutoTagsSettings';
import { de } from '../i18n/de';
import * as api from '../lib/api/autoTags';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), warn: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined), debug: vi.fn().mockResolvedValue(undefined) }));
vi.mock('../lib/api/autoTags');

const setup = () => render(<LanguageProvider><BlockedAutoTagsSettings /></LanguageProvider>);

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(api.unblockAutoTag).mockReset().mockResolvedValue(undefined);
  vi.mocked(api.listBlockedAutoTags).mockReset();
});

describe('BlockedAutoTagsSettings', () => {
  it('lists the blocked tags with their names in the other languages and a button per tag', async () => {
    vi.mocked(api.listBlockedAutoTags).mockResolvedValue(['miniatur', 'kabelhalter', 'mehrfarbig']);
    setup();
    const rows = await screen.findAllByRole('listitem');
    expect(rows).toHaveLength(3);
    expect(rows[0]).toHaveTextContent('#miniatur');
    expect(rows[1]).toHaveTextContent('#kabelhalter');
    expect(rows[1]).not.toHaveTextContent('auch gesperrt');
    expect(rows[2]).toHaveTextContent('#mehrfarbig');
    expect(rows[2]).toHaveTextContent('auch gesperrt: multicolor');
    expect(within(rows[0]).getByRole('button', { name: 'Sperre für #miniatur aufheben' })).toHaveTextContent(de.blockedAutoTagUnblock);
  });

  it('lifts a block, reloads the list and moves focus to the next row', async () => {
    vi.mocked(api.listBlockedAutoTags).mockResolvedValueOnce(['miniatur', 'kabelhalter']).mockResolvedValue(['kabelhalter']);
    setup();
    fireEvent.click(await screen.findByRole('button', { name: 'Sperre für #miniatur aufheben' }));
    await waitFor(() => expect(api.unblockAutoTag).toHaveBeenCalledWith('miniatur'));
    await waitFor(() => expect(screen.queryByText('#miniatur')).toBeNull());
    expect(screen.getByRole('button', { name: 'Sperre für #kabelhalter aufheben' })).toHaveFocus();
  });

  it('shows the explanation instead of an empty list and keeps focus on it after the last block is lifted', async () => {
    vi.mocked(api.listBlockedAutoTags).mockResolvedValueOnce(['miniatur']).mockResolvedValue([]);
    setup();
    expect(screen.getByText(de.blockedAutoTagsTitle)).toBeInTheDocument();
    fireEvent.click(await screen.findByRole('button', { name: 'Sperre für #miniatur aufheben' }));
    const empty = await screen.findByText(de.blockedAutoTagsEmpty);
    await waitFor(() => expect(empty).toHaveFocus());
    expect(screen.queryByRole('list')).toBeNull();
  });

  it('shows the empty state right away when nothing is blocked', async () => {
    vi.mocked(api.listBlockedAutoTags).mockResolvedValue([]);
    setup();
    expect(await screen.findByText(de.blockedAutoTagsEmpty)).toBeInTheDocument();
    expect(screen.getByText(de.blockedAutoTagsHint)).toBeInTheDocument();
  });

  it('keeps the row and reports the problem when lifting fails', async () => {
    vi.mocked(api.listBlockedAutoTags).mockResolvedValue(['miniatur']);
    vi.mocked(api.unblockAutoTag).mockRejectedValue(new Error('Datenbank gesperrt'));
    setup();
    fireEvent.click(await screen.findByRole('button', { name: 'Sperre für #miniatur aufheben' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(de.autoTagBlockError);
    expect(screen.getByText('#miniatur')).toBeInTheDocument();
  });

  it('says so when the list cannot be loaded instead of claiming nothing is blocked', async () => {
    vi.mocked(api.listBlockedAutoTags).mockRejectedValue(new Error('x'));
    setup();
    expect(await screen.findByRole('alert')).toHaveTextContent(de.blockedAutoTagsLoadError);
    expect(screen.queryByText(de.blockedAutoTagsEmpty)).toBeNull();
  });
});
