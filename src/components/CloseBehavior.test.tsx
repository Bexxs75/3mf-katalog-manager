import React from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { LanguageProvider, useLanguage } from '../i18n/LanguageContext';
import { DiagnosticsProvider } from '../diagnostics/DiagnosticsContext';
import { CloseBehaviorSettings } from './CloseBehaviorSettings';
import { TrayLabelSync } from './TrayLabelSync';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import * as api from '../lib/api/closeBehavior';

vi.mock('../lib/api/closeBehavior', async (importActual) => ({
  ...(await importActual<typeof import('../lib/api/closeBehavior')>()),
  getCloseBehavior: vi.fn(),
  setCloseBehavior: vi.fn(),
  setTrayLabels: vi.fn(),
}));

const base: api.CloseBehavior = { action: 'exit', trayAvailable: false, supported: true, platform: 'windows' };

function wrap(node: React.ReactNode) {
  return render(<LanguageProvider><DiagnosticsProvider>{node}</DiagnosticsProvider></LanguageProvider>);
}

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(api.getCloseBehavior).mockReset();
  vi.mocked(api.setCloseBehavior).mockReset();
  vi.mocked(api.setTrayLabels).mockReset().mockResolvedValue(undefined);
});

describe('normalizeCloseBehavior', () => {
  it('treats anything but a supported reply as not supported and cleans up unknown values', async () => {
    const { normalizeCloseBehavior } = await vi.importActual<typeof import('../lib/api/closeBehavior')>('../lib/api/closeBehavior');
    expect(normalizeCloseBehavior(null)).toBeNull();
    expect(normalizeCloseBehavior([])).toBeNull();
    expect(normalizeCloseBehavior({ supported: false, action: 'tray' })).toBeNull();
    expect(normalizeCloseBehavior({ supported: true, action: 'minimize', platform: 'amiga' }))
      .toEqual({ action: 'exit', trayAvailable: false, supported: true, platform: 'other' });
    expect(normalizeCloseBehavior({ supported: true, action: 'tray', trayAvailable: true, platform: 'linux' }))
      .toEqual({ action: 'tray', trayAvailable: true, supported: true, platform: 'linux' });
  });
});

describe('CloseBehaviorSettings', () => {
  it('shows nothing where the backend reports no notification area', async () => {
    vi.mocked(api.getCloseBehavior).mockResolvedValue(null);
    const { container } = wrap(<CloseBehaviorSettings />);
    await waitFor(() => expect(api.getCloseBehavior).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });

  it('loads the state, saves a change and keeps the keyboard-reachable radios', async () => {
    vi.mocked(api.getCloseBehavior).mockResolvedValue(base);
    vi.mocked(api.setCloseBehavior).mockResolvedValue({ ...base, action: 'tray', trayAvailable: true });
    wrap(<CloseBehaviorSettings />);
    const exit = await screen.findByRole('radio', { name: de.closeActionExit });
    const tray = screen.getByRole('radio', { name: de.closeActionTray });
    expect(screen.getByRole('group', { name: de.closeActionTitle })).toBeInTheDocument();
    expect(exit).toBeChecked();
    expect(tray).not.toBeChecked();
    expect(screen.getByText(de.closeActionHelp)).toBeInTheDocument();
    expect(screen.queryByText(de.closeActionLinuxWarning)).toBeNull();
    fireEvent.click(tray);
    await waitFor(() => expect(tray).toBeChecked());
    expect(api.setCloseBehavior).toHaveBeenCalledWith('tray');
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('uses the menu-bar wording on macOS and the warning on Linux only', async () => {
    vi.mocked(api.getCloseBehavior).mockResolvedValue({ ...base, platform: 'macos' });
    const mac = wrap(<CloseBehaviorSettings />);
    expect(await screen.findByRole('radio', { name: de.closeActionTrayMac })).toBeInTheDocument();
    expect(screen.getByText(de.closeActionHelpMac)).toBeInTheDocument();
    expect(screen.queryByText(de.closeActionLinuxWarning)).toBeNull();
    mac.unmount();

    vi.mocked(api.getCloseBehavior).mockResolvedValue({ ...base, platform: 'linux' });
    wrap(<CloseBehaviorSettings />);
    expect(await screen.findByRole('radio', { name: de.closeActionTrayLinux })).toBeInTheDocument();
    expect(screen.getByText(de.closeActionLinuxWarning)).toBeInTheDocument();
  });

  it('falls back to "quit" in the display and says so when the icon could not be built', async () => {
    vi.mocked(api.getCloseBehavior).mockResolvedValue(base);
    vi.mocked(api.setCloseBehavior).mockResolvedValue({ ...base, action: 'tray', trayAvailable: false });
    wrap(<CloseBehaviorSettings />);
    fireEvent.click(await screen.findByRole('radio', { name: de.closeActionTray }));
    expect(await screen.findByRole('alert')).toHaveTextContent(de.closeActionUnavailable);
    expect(screen.getByRole('radio', { name: de.closeActionExit })).toBeChecked();
    expect(screen.getByRole('radio', { name: de.closeActionTray })).not.toBeChecked();
  });

  it('reports a failed save and leaves the old choice', async () => {
    vi.mocked(api.getCloseBehavior).mockResolvedValue(base);
    vi.mocked(api.setCloseBehavior).mockRejectedValue({ message: 'boom', expected: true });
    wrap(<CloseBehaviorSettings />);
    fireEvent.click(await screen.findByRole('radio', { name: de.closeActionTray }));
    expect(await screen.findByRole('alert')).toHaveTextContent(de.closeActionFailed);
    expect(screen.getByRole('radio', { name: de.closeActionExit })).toBeChecked();
  });
});

describe('TrayLabelSync', () => {
  it('sends the menu texts at start and again in the new language', async () => {
    function SwitchToEnglish() {
      return <button onClick={() => setLanguage('en')}>en</button>;
    }
    let setLanguage: (language: 'en') => void = () => {};
    function Capture() {
      setLanguage = useLanguage().setLanguage;
      return <SwitchToEnglish />;
    }
    wrap(<><TrayLabelSync /><Capture /></>);
    await waitFor(() => expect(api.setTrayLabels).toHaveBeenCalledWith(de.closeActionTrayOpen, de.closeActionTrayQuit));
    fireEvent.click(screen.getByText('en'));
    await waitFor(() => expect(api.setTrayLabels).toHaveBeenLastCalledWith(en.closeActionTrayOpen, en.closeActionTrayQuit));
  });
});
