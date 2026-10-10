import { RuntimeEnvironmentProvider } from "../hooks/useRuntimeEnvironment";
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { InfoFolders } from './InfoFolders';
import * as diagnosticsApi from '../lib/api/diagnostics';

vi.mock('../lib/api/diagnostics');

const openBugReport = vi.fn();

// Plain factory for the same reason as in DiagnosticsSettings.test.tsx: the
// nested report link must see the mock, not the real provider hook.
vi.mock('./DiagnosticsContext', () => ({
  useDiagnostics: () => ({ openBugReport }),
}));

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(diagnosticsApi.openDataFolder).mockResolvedValue(undefined);
  vi.mocked(diagnosticsApi.openLogFolder).mockResolvedValue(undefined);
});

function renderFolders() {
  render(
    <LanguageProvider>
      <InfoFolders />
    </LanguageProvider>,
  );
}

describe('InfoFolders', () => {
  it('shows both buttons with their hints', () => {
    renderFolders();
    expect(screen.getByText('ORDNER')).toBeInTheDocument();
    expect(screen.getByText('Katalog, Sicherungen vor Updates')).toBeInTheDocument();
    expect(screen.getByText('Logdateien')).toBeInTheDocument();
  });

  it('opens the data folder', async () => {
    renderFolders();
    fireEvent.click(screen.getByText('Datenordner öffnen'));
    await waitFor(() => expect(diagnosticsApi.openDataFolder).toHaveBeenCalled());
    expect(diagnosticsApi.openLogFolder).not.toHaveBeenCalled();
  });

  it('opens the log folder', async () => {
    renderFolders();
    fireEvent.click(screen.getByText('Log-Ordner öffnen'));
    await waitFor(() => expect(diagnosticsApi.openLogFolder).toHaveBeenCalled());
    expect(diagnosticsApi.openDataFolder).not.toHaveBeenCalled();
  });

  it('shows the error and a report link when opening fails unexpectedly', async () => {
    vi.mocked(diagnosticsApi.openDataFolder).mockRejectedValue({ message: 'kein Dateimanager', expected: false });
    renderFolders();
    fireEvent.click(screen.getByText('Datenordner öffnen'));
    await waitFor(() => expect(screen.getByText('kein Dateimanager')).toBeInTheDocument());
    expect(screen.getByText('Problem melden')).toBeInTheDocument();
  });

  it('shows an expected error without a report link', async () => {
    vi.mocked(diagnosticsApi.openLogFolder).mockRejectedValue({ message: 'Ordner nicht gefunden', expected: true });
    renderFolders();
    fireEvent.click(screen.getByText('Log-Ordner öffnen'));
    await waitFor(() => expect(screen.getByText('Ordner nicht gefunden')).toBeInTheDocument());
    expect(screen.queryByText('Problem melden')).not.toBeInTheDocument();
  });
});

it('shows container paths and volume guidance instead of folder buttons', async () => {
  vi.mocked(diagnosticsApi.getDataPaths).mockResolvedValue({ data: '/config/data/catalog', logs: '/config/data/catalog/logs' });
  render(<LanguageProvider><RuntimeEnvironmentProvider value={{ container: true }}><InfoFolders /></RuntimeEnvironmentProvider></LanguageProvider>);
  expect(await screen.findByText(/Daten unter \/config\/data\/catalog\./)).toHaveTextContent('Volume /config');
  expect(screen.getByText(/Logs unter \/config\/data\/catalog\/logs\./)).toBeInTheDocument();
  expect(screen.queryByRole('button')).not.toBeInTheDocument();
  expect(diagnosticsApi.openDataFolder).not.toHaveBeenCalled();
  expect(diagnosticsApi.openLogFolder).not.toHaveBeenCalled();
});
