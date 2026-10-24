import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { AutoTaggingSetting } from './AutoTaggingSetting';
import { de } from '../i18n/de';
import * as api from '../lib/api/autoTags';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), warn: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined), debug: vi.fn().mockResolvedValue(undefined) }));
vi.mock('../lib/api/autoTags');


const setup = () => render(<LanguageProvider><AutoTaggingSetting /></LanguageProvider>);

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(api.getAutoTagging).mockReset().mockResolvedValue(true);
  vi.mocked(api.listBlockedAutoTags).mockReset().mockResolvedValue(['miniatur']);
  vi.mocked(api.setAutoTagging).mockReset().mockResolvedValue(undefined);
});

describe('AutoTaggingSetting', () => {
  it('is a labelled switch that is on by default and explains the effect', async () => {
    setup();
    const box = await screen.findByRole('switch', { name: de.autoTaggingLabel });
    expect(box).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByText(de.autoTaggingHint)).toBeInTheDocument();
  });

  it('shows the stored off state', async () => {
    vi.mocked(api.getAutoTagging).mockResolvedValue(false);
    setup();
    await waitFor(() => expect(screen.getByRole('switch')).toHaveAttribute('aria-checked', 'false'));
  });

  it('saves the change and keeps the new state', async () => {
    setup();
    fireEvent.click(await screen.findByRole('switch'));
    await waitFor(() => expect(api.setAutoTagging).toHaveBeenCalledWith(false));
    await waitFor(() => expect(screen.getByRole('switch')).toHaveAttribute('aria-checked', 'false'));
  });

  it('keeps the old state and reports the problem when saving fails', async () => {
    vi.mocked(api.setAutoTagging).mockRejectedValue(new Error('boom'));
    setup();
    fireEvent.click(await screen.findByRole('switch'));
    expect(await screen.findByRole('alert')).toHaveTextContent(de.autoTaggingError);
    expect(screen.getByRole('switch')).toHaveAttribute('aria-checked', 'true');
  });

  it('dims the blocked-tags section while off but keeps it usable', async () => {
    vi.mocked(api.getAutoTagging).mockResolvedValue(false);
    setup();
    expect(await screen.findByText(de.autoTaggingBlockedNote)).toBeInTheDocument();
    const unblock = await screen.findByRole('button', { name: 'Sperre für #miniatur aufheben' });
    expect(unblock).toBeEnabled();
    expect(unblock.closest('[class*="opacity-"]')).not.toBeNull();
  });
});
