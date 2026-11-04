import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { ThingiverseView } from './ThingiverseView';
import { de } from '../i18n/de';
import * as api from '../lib/api/thingiverse';
import type { ThingDetail, ThingSummary, ThingiverseSettings } from '../types';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), warn: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined), debug: vi.fn().mockResolvedValue(undefined) }));
vi.mock('../lib/api/thingiverse');

const onOpenSettings = vi.fn();
const writeText = vi.fn();
let settings: ThingiverseSettings;

const thing = (id: number, extra: Partial<ThingSummary> = {}): ThingSummary => ({
  id, name: `Thing ${id}`, authorName: `Author ${id}`, licenseName: 'CC BY 4.0', likeCount: id, previewAvailable: true,
  url: `https://www.thingiverse.com/thing:${id}`, ...extra,
});
const detailOf = (t: ThingSummary, extra: Partial<ThingDetail> = {}): ThingDetail => ({
  ...t, description: 'Plain <b>text</b>\nsecond line', createdAt: '2025-03-12T10:00:00+00:00', tags: ['cable', 'holder'], ...extra,
});

function setup() {
  return render(<LanguageProvider><ThingiverseView onOpenSettings={onOpenSettings} /></LanguageProvider>);
}
const searchBox = () => screen.getByRole('searchbox', { name: de.tvSearchLabel });
async function search(term = 'kabel') {
  fireEvent.change(await screen.findByRole('searchbox', { name: de.tvSearchLabel }), { target: { value: term } });
  fireEvent.click(screen.getByRole('button', { name: de.tvSearchButton }));
}

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  settings = { enabled: true, hasToken: true, tokenMask: '••••a3f9', usageToday: 12, dailyLimit: 1000 };
  onOpenSettings.mockReset();
  writeText.mockReset().mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
  vi.mocked(api.getThingiverseSettings).mockReset().mockImplementation(async () => ({ ...settings }));
  vi.mocked(api.thingiverseSearch).mockReset().mockResolvedValue({ items: [thing(1), thing(2, { licenseName: null, authorName: '' })], hasMore: false, page: 1 });
  vi.mocked(api.thingiverseThing).mockReset().mockImplementation(async id => detailOf(thing(id)));
  vi.mocked(api.thingiverseImage).mockReset().mockResolvedValue('data:image/png;base64,AAAA');
  vi.mocked(api.openThingiverseThing).mockReset().mockResolvedValue(undefined);
});
afterEach(() => vi.useRealTimers());

describe('ThingiverseView without a key', () => {
  it('explains the setup, sends no request and offers the settings', async () => {
    settings = { ...settings, hasToken: false, tokenMask: null };
    setup();
    expect(await screen.findByText(de.tvNoKeyTitle)).toBeInTheDocument();
    expect(screen.queryByRole('searchbox')).toBeNull();
    expect(screen.getByText(de.tvNoKeyFooter)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: de.tvOpenSettings }));
    expect(onOpenSettings).toHaveBeenCalled();
    expect(api.thingiverseSearch).not.toHaveBeenCalled();
  });
});

describe('ThingiverseView search', () => {
  it('starts empty and searches only on submit, never while typing or on opening', async () => {
    setup();
    expect(await screen.findByText(de.tvEmptyTitle)).toBeInTheDocument();
    expect(searchBox()).toHaveValue('');
    fireEvent.change(searchBox(), { target: { value: 'kab' } });
    fireEvent.change(searchBox(), { target: { value: 'kabel' } });
    expect(api.thingiverseSearch).not.toHaveBeenCalled();
    fireEvent.submit(searchBox().closest('form')!);
    await waitFor(() => expect(api.thingiverseSearch).toHaveBeenCalledWith('kabel', 1));
  });

  it('does not search for an empty term', async () => {
    setup();
    await search('   ');
    expect(api.thingiverseSearch).not.toHaveBeenCalled();
  });

  it('shows each hit with its author, license and the three actions but no download or save', async () => {
    setup();
    await search();
    const first = await screen.findByRole('article', { name: 'Thing 1' });
    expect(within(first).getByText('von Author 1')).toBeInTheDocument();
    expect(within(first).getByText('CC BY 4.0')).toBeInTheDocument();
    expect(within(first).getByRole('button', { name: new RegExp(de.tvDetails) })).toBeInTheDocument();
    expect(within(first).getByRole('button', { name: new RegExp(de.tvOpen) })).toBeInTheDocument();
    expect(within(first).getByRole('button', { name: new RegExp(de.tvCopyLink) })).toBeInTheDocument();
    const second = screen.getByRole('article', { name: 'Thing 2' });
    expect(within(second).getByText(de.tvAuthorUnknown)).toBeInTheDocument();
    expect(within(second).getByText(de.tvLicenseNone)).toBeInTheDocument();
    expect(screen.getByText('2 Treffer für „kabel“')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /herunterladen|download|speichern|katalog/i })).toBeNull();
  });

  it('loads the preview images of visible hits as pictures and releases them on leaving', async () => {
    const { unmount } = setup();
    await search();
    await waitFor(() => expect(document.querySelectorAll('img').length).toBe(2));
    expect(api.thingiverseImage).toHaveBeenCalledWith(1, 'thumb');
    expect(api.thingiverseImage).toHaveBeenCalledTimes(2);
    unmount();
    expect(document.querySelectorAll('img').length).toBe(0);
  });

  it('shows a placeholder for a hit without a preview and does not ask for one', async () => {
    vi.mocked(api.thingiverseSearch).mockResolvedValue({ items: [thing(5, { previewAvailable: false })], hasMore: false, page: 1 });
    setup();
    await search();
    await screen.findByRole('article', { name: 'Thing 5' });
    expect(screen.getByText(de.tvNoImage)).toBeInTheDocument();
    expect(api.thingiverseImage).not.toHaveBeenCalled();
  });

  it('says so when nothing was found', async () => {
    vi.mocked(api.thingiverseSearch).mockResolvedValue({ items: [], hasMore: false, page: 1 });
    setup();
    await search('xyz');
    expect(await screen.findByText(de.tvNoResultsTitle)).toBeInTheDocument();
    expect(screen.getByText(/Für „xyz“ hat Thingiverse nichts gefunden/)).toBeInTheDocument();
  });

  it('appends the next page on "Mehr laden" with one request', async () => {
    vi.mocked(api.thingiverseSearch)
      .mockResolvedValueOnce({ items: [thing(1)], hasMore: true, page: 1 })
      .mockResolvedValueOnce({ items: [thing(2)], hasMore: false, page: 2 });
    setup();
    await search();
    fireEvent.click(await screen.findByRole('button', { name: de.tvLoadMore }));
    await screen.findByRole('article', { name: 'Thing 2' });
    expect(api.thingiverseSearch).toHaveBeenLastCalledWith('kabel', 2);
    expect(screen.getByRole('article', { name: 'Thing 1' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: de.tvLoadMore })).toBeNull();
  });

  it('shows the day counter and the privacy line in the footer', async () => {
    setup();
    expect(await screen.findByText('Heute etwa 12 von ungefähr 1000 Abfragen (Schätzung)')).toBeInTheDocument();
    expect(screen.getByText(de.tvFootPrivacy)).toBeInTheDocument();
  });

  it('copies the link of a hit and says where to paste it', async () => {
    setup();
    await search();
    const first = await screen.findByRole('article', { name: 'Thing 1' });
    fireEvent.click(within(first).getByRole('button', { name: new RegExp(de.tvCopyLink) }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('https://www.thingiverse.com/thing:1'));
    expect(await screen.findByText(de.tvLinkCopied)).toBeInTheDocument();
  });

  it('opens a hit in the browser by its number, never by an address from the page', async () => {
    setup();
    await search();
    const first = await screen.findByRole('article', { name: 'Thing 1' });
    fireEvent.click(within(first).getByRole('button', { name: new RegExp(de.tvOpen) }));
    expect(api.openThingiverseThing).toHaveBeenCalledWith(1);
  });
});

describe('ThingiverseView errors', () => {
  const cases: [string, string, string][] = [
    ['thingiverseOffline', de.tvTitleOffline, de.errThingiverseOffline],
    ['thingiverseTimeout', de.tvTitleOffline, de.errThingiverseTimeout],
    ['thingiverseTokenInvalid', de.tvTitleInvalid, de.errThingiverseTokenInvalid],
    ['thingiverseBlocked', de.tvTitleBlocked, de.errThingiverseBlocked],
    ['thingiverseBadResponse', de.tvTitleBad, de.errThingiverseBadResponse],
    ['thingiverseDailyLimit', de.tvTitleDaily, de.errThingiverseDailyLimit],
    ['thingiverseQueryInvalid', de.tvTitleQuery, de.errThingiverseQueryInvalid],
    ['thingiverseNoToken', de.tvTitleNotSetUp, de.errThingiverseNoToken],
    ['thingiverseDisabled', de.tvTitleNotSetUp, de.errThingiverseDisabled],
  ];
  it.each(cases)('%s gets a banner with title and cause', async (code, title, text) => {
    vi.mocked(api.thingiverseSearch).mockRejectedValue({ message: code, expected: true });
    setup();
    await search();
    const banner = await screen.findByText(title);
    expect(banner.closest('[role]')).toHaveTextContent(text);
  });

  it('keeps the results visible when the network fails and retries on request', async () => {
    setup();
    await search();
    await screen.findByRole('article', { name: 'Thing 1' });
    vi.mocked(api.thingiverseSearch).mockRejectedValueOnce({ message: 'thingiverseOffline', expected: true });
    fireEvent.click(screen.getByRole('button', { name: de.tvSearchButton }));
    expect(await screen.findByText(de.tvTitleOffline)).toBeInTheDocument();
    expect(screen.getByRole('article', { name: 'Thing 1' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: de.tvRetry }));
    await waitFor(() => expect(screen.queryByText(de.tvTitleOffline)).toBeNull());
    expect(api.thingiverseSearch).toHaveBeenCalledTimes(3);
  });

  it('points to the settings when the key was refused', async () => {
    vi.mocked(api.thingiverseSearch).mockRejectedValue({ message: 'thingiverseTokenInvalid', expected: true });
    setup();
    await search();
    fireEvent.click(await screen.findByRole('button', { name: de.tvOpenSettings }));
    expect(onOpenSettings).toHaveBeenCalled();
  });

  it('blocks the search button for good once the day is used up', async () => {
    vi.mocked(api.thingiverseSearch).mockRejectedValue({ message: 'thingiverseDailyLimit', expected: true });
    setup();
    await search();
    await screen.findByText(de.tvTitleDaily);
    expect(screen.getByRole('button', { name: de.tvSearchButton })).toBeDisabled();
  });

  it('counts a rate limit down, blocks the button meanwhile and lifts the banner at zero', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.mocked(api.thingiverseSearch).mockRejectedValue({ message: 'thingiverseRateLimited', expected: true, params: ['3'] });
    setup();
    await search();
    expect(await screen.findByText('Bitte warte noch 3 Sekunden.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: de.tvSearchButton })).toBeDisabled();
    await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
    expect(screen.getByText('Bitte warte noch 2 Sekunden.')).toBeInTheDocument();
    // One tick per second: every step re-renders and arms the next timer.
    for (let i = 0; i < 2; i++) await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
    await waitFor(() => expect(screen.queryByText(de.tvTitleRate)).toBeNull());
    expect(screen.getByRole('button', { name: de.tvSearchButton })).toBeEnabled();
  });
});

describe('ThingiverseView detail', () => {
  async function openDetail() {
    setup();
    await search();
    const first = await screen.findByRole('article', { name: 'Thing 1' });
    fireEvent.click(within(first).getByRole('button', { name: new RegExp(de.tvDetails) }));
    await screen.findByText(de.tvNoDownload);
  }

  it('shows author, license, plain-text description and tags, and says that downloading happens on Thingiverse', async () => {
    await openDetail();
    expect(api.thingiverseThing).toHaveBeenCalledWith(1);
    expect(screen.getByText('von Author 1')).toBeInTheDocument();
    expect(screen.getByText('Lizenz: CC BY 4.0')).toBeInTheDocument();
    expect(screen.getByText(de.tvLicenseNotice)).toBeInTheDocument();
    // The description is text: markup in it is shown as it is and creates no element.
    const description = screen.getByText(/Plain <b>text<\/b>/);
    expect(description.querySelector('b')).toBeNull();
    expect(screen.getByText('cable')).toBeInTheDocument();
    expect(screen.getByText(de.tvTagsNote)).toBeInTheDocument();
    expect(screen.getByText('2025-03-12')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /herunterladen|download|speichern/i })).toBeNull();
    await waitFor(() => expect(api.thingiverseImage).toHaveBeenCalledWith(1, 'large'));
  });

  it('says when the license is missing', async () => {
    vi.mocked(api.thingiverseThing).mockResolvedValue(detailOf(thing(1, { licenseName: null })));
    await openDetail();
    expect(screen.getByText(de.tvLicenseMissingLine)).toBeInTheDocument();
    expect(screen.getByText(de.tvLicenseMissingText)).toBeInTheDocument();
  });

  it('returns to the list with the hits and the term still there', async () => {
    await openDetail();
    fireEvent.click(screen.getByRole('button', { name: new RegExp(de.tvBackToList) }));
    expect(await screen.findByRole('article', { name: 'Thing 2' })).toBeInTheDocument();
    expect(searchBox()).toHaveValue('kabel');
    expect(api.thingiverseSearch).toHaveBeenCalledTimes(1);
  });

  it('explains a thing that is gone and offers the way back', async () => {
    vi.mocked(api.thingiverseThing).mockRejectedValue({ message: 'thingiverseNotFound', expected: true });
    setup();
    await search();
    const first = await screen.findByRole('article', { name: 'Thing 1' });
    fireEvent.click(within(first).getByRole('button', { name: new RegExp(de.tvDetails) }));
    expect(await screen.findByText(de.tvTitleNotFound)).toBeInTheDocument();
    const banner = screen.getByText(de.tvTitleNotFound).closest('[role]') as HTMLElement;
    fireEvent.click(within(banner).getByRole('button', { name: de.tvBackToList }));
    expect(await screen.findByRole('article', { name: 'Thing 2' })).toBeInTheDocument();
  });

  it('copies the link from the detail page', async () => {
    await openDetail();
    fireEvent.click(screen.getByRole('button', { name: de.tvCopyLink }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('https://www.thingiverse.com/thing:1'));
  });
});
