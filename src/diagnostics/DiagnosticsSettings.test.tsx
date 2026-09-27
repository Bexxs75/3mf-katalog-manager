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
});
