import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { DiagnosticsSettings } from './DiagnosticsSettings';
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

  it('shows the error without a report link when opening the log folder fails with an expected error', async () => {
    vi.mocked(diagnosticsApi.openLogFolder).mockRejectedValue({ message: 'Ordner nicht gefunden', expected: true });
    renderSettings();
    fireEvent.click(screen.getByText('Ordner öffnen'));
    await waitFor(() => expect(screen.getByText('Ordner nicht gefunden')).toBeInTheDocument());
    expect(screen.queryByText('Problem melden')).not.toBeInTheDocument();
  });

  it('shows the error and a report link when opening the log folder fails unexpectedly', async () => {
    vi.mocked(diagnosticsApi.openLogFolder).mockRejectedValue({ message: 'Ordner nicht gefunden', expected: false });
    renderSettings();
    fireEvent.click(screen.getByText('Ordner öffnen'));
    await waitFor(() => expect(screen.getByText('Ordner nicht gefunden')).toBeInTheDocument());

    const reportLink = screen.getByText('Problem melden');
    expect(reportLink).toBeInTheDocument();
    fireEvent.click(reportLink);
    expect(openBugReport).toHaveBeenCalled();
  });
});
