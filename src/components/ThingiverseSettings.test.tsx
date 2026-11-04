import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { ThingiverseSettings } from './ThingiverseSettings';
import { de } from '../i18n/de';
import * as api from '../lib/api/thingiverse';
import type { ThingiverseSettings as Settings } from '../types';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), warn: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined), debug: vi.fn().mockResolvedValue(undefined) }));
vi.mock('../lib/api/thingiverse');

const KEY = 'abcd1234efgh5678a3f9';
let stored: Settings;
const onEnabledChange = vi.fn();

function setup() {
  return render(<LanguageProvider><ThingiverseSettings onEnabledChange={onEnabledChange} /></LanguageProvider>);
}
const field = () => screen.getByLabelText(de.tvTokenLabel) as HTMLInputElement;

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  stored = { enabled: false, hasToken: false, tokenMask: null, usageToday: 0, dailyLimit: 1000 };
  onEnabledChange.mockReset();
  vi.mocked(api.getThingiverseSettings).mockReset().mockImplementation(async () => ({ ...stored }));
  vi.mocked(api.setThingiverseEnabled).mockReset().mockImplementation(async enabled => { stored = { ...stored, enabled }; });
  vi.mocked(api.testThingiverseToken).mockReset().mockImplementation(async () => {
    stored = { ...stored, hasToken: true, tokenMask: '••••a3f9' };
    return { ok: true, userName: 'Ann' };
  });
  vi.mocked(api.removeThingiverseToken).mockReset().mockImplementation(async () => { stored = { ...stored, hasToken: false, tokenMask: null }; });
});

describe('ThingiverseSettings', () => {
  it('is off by default and shows no key field, but always the privacy note', async () => {
    setup();
    const toggle = await screen.findByRole('switch', { name: de.tvSwitchLabel });
    expect(toggle).toHaveAttribute('aria-checked', 'false');
    expect(screen.queryByLabelText(de.tvTokenLabel)).toBeNull();
    expect(screen.getByText(de.tvPrivacy)).toBeInTheDocument();
    expect(screen.getByText(de.tvExperimental)).toBeInTheDocument();
  });

  it('asks before switching on: cancel keeps it off, confirm turns it on and tells the app', async () => {
    setup();
    fireEvent.click(await screen.findByRole('switch'));
    const dialog = screen.getByRole('dialog', { name: de.tvEnableTitle });
    expect(within(dialog).getByText(de.tvEnablePoint2)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: de.cancel }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(api.setThingiverseEnabled).not.toHaveBeenCalled();
    expect(screen.getByRole('switch')).toHaveAttribute('aria-checked', 'false');

    fireEvent.click(screen.getByRole('switch'));
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: de.tvEnableConfirm }));
    await waitFor(() => expect(api.setThingiverseEnabled).toHaveBeenCalledWith(true));
    await waitFor(() => expect(screen.getByRole('switch')).toHaveAttribute('aria-checked', 'true'));
    expect(onEnabledChange).toHaveBeenCalledWith(true);
    expect(field().value).toBe('');
  });

  it('switches off without a dialog', async () => {
    stored.enabled = true;
    setup();
    fireEvent.click(await screen.findByRole('switch'));
    expect(screen.queryByRole('dialog')).toBeNull();
    await waitFor(() => expect(api.setThingiverseEnabled).toHaveBeenCalledWith(false));
    expect(onEnabledChange).toHaveBeenCalledWith(false);
  });

  it('checks and saves a key, empties the field and shows only the mask afterwards', async () => {
    stored.enabled = true;
    setup();
    await screen.findByLabelText(de.tvTokenLabel);
    expect(field()).toHaveAttribute('type', 'password');
    expect(field()).toHaveAttribute('autocomplete', 'off');
    fireEvent.click(screen.getByRole('button', { name: de.tvTokenShow }));
    expect(field()).toHaveAttribute('type', 'text');
    fireEvent.change(field(), { target: { value: KEY } });
    fireEvent.click(screen.getByRole('button', { name: de.tvTokenCheck }));
    await waitFor(() => expect(api.testThingiverseToken).toHaveBeenCalledWith(KEY));
    expect(await screen.findByRole('status')).toHaveTextContent('„Ann“');
    expect(screen.getByLabelText(de.tvTokenMaskLabel)).toHaveTextContent('••••a3f9');
    // The key is gone from the screen: not in a field, not in any text.
    expect(screen.queryByLabelText(de.tvTokenLabel)).toBeNull();
    expect(document.body.textContent).not.toContain(KEY);
    expect(document.querySelectorAll('input').length).toBe(0);
  });

  it('does not keep the saved key in the field: replacing starts with an empty field', async () => {
    stored.enabled = true;
    setup();
    await screen.findByLabelText(de.tvTokenLabel);
    fireEvent.change(field(), { target: { value: KEY } });
    fireEvent.click(screen.getByRole('button', { name: de.tvTokenCheck }));
    fireEvent.click(await screen.findByRole('button', { name: de.tvTokenReplace }));
    expect(field().value).toBe('');
    expect(field()).toHaveAttribute('type', 'password');
  });

  it('shows the translated reason when the key is refused and keeps the field for another try', async () => {
    stored.enabled = true;
    vi.mocked(api.testThingiverseToken).mockRejectedValue({ message: 'thingiverseTokenInvalid', expected: true });
    setup();
    await screen.findByLabelText(de.tvTokenLabel);
    fireEvent.change(field(), { target: { value: 'badbadbadbad' } });
    fireEvent.click(screen.getByRole('button', { name: de.tvTokenCheck }));
    expect(await screen.findByRole('alert')).toHaveTextContent(de.errThingiverseTokenInvalid);
    expect(field().value).toBe('badbadbadbad');
    expect(screen.queryByText(de.tvTokenStored)).toBeNull();
  });

  it('never fills the field itself, even when a key is stored; replace opens an empty field', async () => {
    stored = { enabled: true, hasToken: true, tokenMask: '••••a3f9', usageToday: 12, dailyLimit: 1000 };
    setup();
    expect(await screen.findByText(de.tvTokenStored)).toBeInTheDocument();
    expect(screen.queryByLabelText(de.tvTokenLabel)).toBeNull();
    expect(screen.getByText(/Heute etwa 12 von ungefähr 1000 Abfragen/)).toBeInTheDocument();
    expect(screen.queryByText(de.tvUsageWarn)).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: de.tvTokenReplace }));
    expect(field().value).toBe('');
  });

  it('warns when the day is nearly used up', async () => {
    stored = { enabled: true, hasToken: true, tokenMask: '••••a3f9', usageToday: 850, dailyLimit: 1000 };
    setup();
    expect(await screen.findByText(de.tvUsageWarn)).toBeInTheDocument();
  });

  it('removes the key only after a confirmation that says it stays valid at Thingiverse', async () => {
    stored = { enabled: true, hasToken: true, tokenMask: '••••a3f9', usageToday: 0, dailyLimit: 1000 };
    setup();
    fireEvent.click(await screen.findByRole('button', { name: de.tvTokenRemove }));
    let dialog = screen.getByRole('dialog', { name: de.tvRemoveTitle });
    expect(within(dialog).getByText(de.tvRemoveText)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: de.cancel }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(api.removeThingiverseToken).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: de.tvTokenRemove }));
    dialog = screen.getByRole('dialog', { name: de.tvRemoveTitle });
    fireEvent.click(within(dialog).getByRole('button', { name: de.tvTokenRemove }));
    await waitFor(() => expect(api.removeThingiverseToken).toHaveBeenCalledTimes(1));
    expect(await screen.findByLabelText(de.tvTokenLabel)).toBeInTheDocument();
    expect(screen.queryByText(de.tvTokenStored)).toBeNull();
  });

  it('says that the key stays on this computer and is in no backup', async () => {
    stored = { enabled: true, hasToken: true, tokenMask: '••••a3f9', usageToday: 0, dailyLimit: 1000 };
    setup();
    expect(await screen.findByText(de.tvKeptOnRestore)).toBeInTheDocument();
  });

  it('unfolds and folds the instructions', async () => {
    stored.enabled = true;
    setup();
    const toggle = await screen.findByRole('button', { name: new RegExp(de.tvHowTitle) });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByText(de.tvHowStep1)).toBeNull();
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText(de.tvHowStep1)).toBeInTheDocument();
    expect(screen.getByText(de.tvHowMayChange)).toBeInTheDocument();
    expect(screen.getByText(de.tvHowStep4)).toHaveTextContent('App Token');
    fireEvent.click(toggle);
    expect(screen.queryByText(de.tvHowStep1)).toBeNull();
  });
});
