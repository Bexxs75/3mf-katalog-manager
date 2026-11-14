import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { DiagnosticsSettings } from './DiagnosticsSettings';
import { de } from '../i18n/de';
import * as diagnosticsApi from '../lib/api/diagnostics';

vi.mock('../lib/api/diagnostics');

const openBugReport = vi.fn();

// A plain (non-async) factory: an async factory that spreads `importActual`'s
// result does not reliably apply to every consumer of this module - a nested
// component (ReportProblemLink, reached via ErrorText) kept getting the real
// implementation and threw "outside DiagnosticsProvider". No provider or
// other export from this module is needed by anything under test here.
vi.mock('./DiagnosticsContext', () => ({
  useDiagnostics: () => ({ openBugReport }),
}));

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  openBugReport.mockClear();
  vi.mocked(diagnosticsApi.getVerboseLogging).mockResolvedValue({ enabled: false, untilMs: null });
  vi.mocked(diagnosticsApi.setVerboseLogging).mockResolvedValue({ enabled: true, untilMs: Date.parse('2026-10-03') });
});

function renderSettings() {
  render(
    <LanguageProvider>
      <DiagnosticsSettings />
    </LanguageProvider>,
  );
}

describe('DiagnosticsSettings states', () => {
  it('does not guess the switch when the stored state cannot be read', async () => {
    vi.mocked(diagnosticsApi.getVerboseLogging).mockRejectedValueOnce(new Error('boom')).mockResolvedValue({ enabled: true, untilMs: Date.parse('2026-10-03') });
    renderSettings();
    expect(await screen.findByRole('alert')).toHaveTextContent(de.settingLoadFailed);
    expect(screen.getByRole('switch')).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: de.settingReload }));
    await waitFor(() => expect(screen.getByRole('switch')).toBeEnabled());
    expect(screen.getByRole('switch')).toHaveAttribute('aria-checked', 'true');
  });

  it('keeps the confirmed state and reports a failed save', async () => {
    vi.mocked(diagnosticsApi.setVerboseLogging).mockRejectedValue(new Error('boom'));
    renderSettings();
    await waitFor(() => expect(screen.getByRole('switch')).toBeEnabled());
    fireEvent.click(screen.getByRole('switch'));
    expect(await screen.findByRole('alert')).toHaveTextContent(de.settingSaveFailedKept);
    expect(screen.getByRole('switch')).toHaveAttribute('aria-checked', 'false');
  });
});

describe('DiagnosticsSettings', () => {
  it('calls openBugReport when the bug button is clicked', () => {
    renderSettings();
    fireEvent.click(screen.getByText('Fehler melden'));
    expect(openBugReport).toHaveBeenCalled();
  });

  it('turns verbose logging on and shows the until-date text', async () => {
    renderSettings();
    await waitFor(() => expect(diagnosticsApi.getVerboseLogging).toHaveBeenCalled());

    fireEvent.click(screen.getByRole('switch'));
    await waitFor(() => expect(diagnosticsApi.setVerboseLogging).toHaveBeenCalledWith(true));
    await waitFor(() => expect(screen.getByText(/Eingeschaltet/)).toBeInTheDocument());
  });
});

it('keeps switch focus during and after a pending save', async () => {
  let finish!: (value: { enabled: boolean; untilMs: null }) => void;
  vi.mocked(diagnosticsApi.setVerboseLogging).mockClear().mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  renderSettings();
  const toggle = await screen.findByRole('switch');
  await waitFor(() => expect(toggle).toBeEnabled());
  toggle.focus();
  fireEvent.keyDown(toggle, { key: ' ' });
  await waitFor(() => expect(toggle).toHaveAttribute('aria-busy', 'true'));
  expect(toggle).toHaveAttribute('aria-disabled', 'true');
  expect(toggle).toBeEnabled();
  expect(document.activeElement).toBe(toggle);
  fireEvent.click(toggle);
  fireEvent.keyDown(toggle, { key: 'Enter' });
  expect(diagnosticsApi.setVerboseLogging).toHaveBeenCalledTimes(1);
  finish({ enabled: true, untilMs: null });
  await waitFor(() => expect(toggle).not.toHaveAttribute('aria-busy'));
  expect(toggle).toHaveAttribute('aria-checked', 'true');
  expect(document.activeElement).toBe(toggle);
});
