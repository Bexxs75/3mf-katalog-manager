import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import App from './App';
import { LanguageProviderWithDiagnostics } from './test/renderWithDiagnostics';
import { UiDensityProvider } from './hooks/UiDensityContext';
import { de } from './i18n/de';
import { makeModelFileSummary } from './test/factories';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), convertFileSrc: (path: string) => path }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: vi.fn().mockResolvedValue(() => {}) }) }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
vi.mock('./components/BackgroundSnapshotRenderer', () => ({ BackgroundSnapshotRenderer: () => null }));
vi.mock('./components/ModelViewer', () => ({ ModelViewer: () => null }));

let tv: { enabled: boolean; hasToken: boolean; tokenMask: string | null; usageToday: number; dailyLimit: number };

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem('3mf-katalog-base-dir', '/old');
  localStorage.setItem('3mf-katalog-setup-seen', '1');
  localStorage.setItem('3mf-katalog-view', 'grid');
  tv = { enabled: true, hasToken: false, tokenMask: null, usageToday: 0, dailyLimit: 1000 };
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'get_app_version') return '0.18.0';
    if (cmd === 'has_step_preview' || cmd === 'is_preview_build' || cmd === 'get_printer_link_enabled') return false;
    if (cmd === 'register_existing_catalog_base_dir' || cmd === 'check_app_update' || cmd === 'get_last_printer_for_file') return null;
    if (cmd === 'get_thingiverse_settings') return { ...tv };
    if (cmd === 'set_thingiverse_enabled') { tv.enabled = (args as { enabled: boolean }).enabled; return null; }
    if (cmd === 'list_file_summaries') return [makeModelFileSummary({ id: 'a', name: 'Alpha.stl' })];
    return [];
  });
});

async function setup() {
  return await act(async () => render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>));
}
const railButton = () => screen.queryByRole('button', { name: de.tvRail });
// The source line is unique to the area; the settings block carries the same heading.
const area = () => screen.queryByText(de.tvAreaSource);

describe('the Thingiverse area in the app', () => {
  it('has no rail entry and no area while the switch is off', async () => {
    tv.enabled = false;
    await setup();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_thingiverse_settings'));
    expect(railButton()).toBeNull();
    expect(area()).toBeNull();
  });

  it('shows the rail entry when the switch is on and opens the area from it', async () => {
    await setup();
    await waitFor(() => expect(railButton()).not.toBeNull());
    fireEvent.click(railButton()!);
    expect(await screen.findByText(de.tvAreaSource)).toBeVisible();
    expect(railButton()).toHaveClass('text-[var(--accent)]');
    // Opening the area asks nothing of Thingiverse by itself.
    expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === 'thingiverse_search' || cmd === 'test_thingiverse_token')).toEqual([]);
  });

  it('leaves the area for the catalog when the switch is turned off while it is open, and does not jump back when turned on again', async () => {
    await setup();
    await waitFor(() => expect(railButton()).not.toBeNull());
    fireEvent.click(railButton()!);
    await screen.findByText(de.tvAreaSource);

    fireEvent.click(screen.getByRole('button', { name: de.settingsTitle }));
    const off = await screen.findByRole('switch', { name: de.tvSwitchLabel });
    fireEvent.click(off);
    await waitFor(() => expect(railButton()).toBeNull());
    expect(area()).toBeNull();
    expect(document.querySelector('[data-model-id="a"]')).toBeInTheDocument();

    fireEvent.click(await screen.findByRole('switch', { name: de.tvSwitchLabel }));
    fireEvent.click(within(await screen.findByRole('dialog', { name: de.tvEnableTitle })).getByRole('button', { name: de.tvEnableConfirm }));
    await waitFor(() => expect(railButton()).not.toBeNull());
    expect(area()).toBeNull();
  });
});
