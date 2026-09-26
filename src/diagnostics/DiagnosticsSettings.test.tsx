import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { DiagnosticsSettings } from './DiagnosticsSettings';
import * as diagnosticsApi from '../lib/api/diagnostics';

vi.mock('../lib/api/diagnostics');

const openBugReport = vi.fn();

vi.mock('./DiagnosticsContext', async () => {
  const actual = await vi.importActual<typeof import('./DiagnosticsContext')>('./DiagnosticsContext');
  return { ...actual, useDiagnostics: () => ({ openBugReport, reportUnexpected: vi.fn() }) };
});

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  openBugReport.mockClear();
  vi.mocked(diagnosticsApi.getVerboseLogging).mockResolvedValue({ enabled: false, untilMs: null });
  vi.mocked(diagnosticsApi.setVerboseLogging).mockResolvedValue({ enabled: true, untilMs: Date.parse('2026-10-03') });
  vi.mocked(diagnosticsApi.openLogFolder).mockResolvedValue(undefined);
});

function renderSettings() {
  render(
    <LanguageProvider>
      <DiagnosticsSettings />
    </LanguageProvider>,
  );
}

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

  it('opens the log folder', async () => {
    renderSettings();
    fireEvent.click(screen.getByText('Ordner öffnen'));
    await waitFor(() => expect(diagnosticsApi.openLogFolder).toHaveBeenCalled());
  });
});
