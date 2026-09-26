import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { BugReportDialog } from './BugReportDialog';
import * as diagnosticsApi from '../lib/api/diagnostics';

vi.mock('../lib/api/diagnostics');

const INFO = { version: '0.15.0', os: 'linux' as const };

function renderDialog(onClose: () => void = vi.fn()) {
  render(
    <LanguageProvider>
      <BugReportDialog onClose={onClose} />
    </LanguageProvider>,
  );
  return onClose;
}

function openButton() {
  return screen.getByText('Formular öffnen').closest('button')!;
}

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(diagnosticsApi.getBugReportInfo).mockResolvedValue(INFO);
  vi.mocked(diagnosticsApi.previewLogExport).mockResolvedValue({
    segments: [{ text: 'plain', replaced: false }],
    containsDebug: false,
    replaceFileNames: true,
    empty: false,
  });
  vi.mocked(diagnosticsApi.saveLogExport).mockResolvedValue('/home/user/Downloads/3mf-katalog-log-2026-09-26.txt');
  vi.mocked(diagnosticsApi.openBugReportForm).mockResolvedValue(undefined);
});

afterEach(() => vi.clearAllMocks());

describe('BugReportDialog', () => {
  it('has no preselected answer and disables the main button', async () => {
    renderDialog();
    await waitFor(() => expect(diagnosticsApi.getBugReportInfo).toHaveBeenCalled());
    const radios = screen.getAllByRole('radio');
    expect(radios).toHaveLength(2);
    radios.forEach((r) => expect(r).not.toBeChecked());
    expect(openButton()).toBeDisabled();
  });

  it('opens the form without a log when "No" is chosen', async () => {
    renderDialog();
    fireEvent.click(screen.getByText('Nein, ohne Logdatei melden'));
    expect(openButton()).not.toBeDisabled();

    fireEvent.click(openButton());
    await waitFor(() => expect(diagnosticsApi.openBugReportForm).toHaveBeenCalledWith('de', false));
    expect(diagnosticsApi.previewLogExport).not.toHaveBeenCalled();
    expect(diagnosticsApi.saveLogExport).not.toHaveBeenCalled();
  });

  it('loads and shows the anonymized preview when "Yes" is chosen', async () => {
    vi.mocked(diagnosticsApi.previewLogExport).mockResolvedValue({
      segments: [
        { text: 'plain text ', replaced: false },
        { text: 'C:\\Users\\Anna', replaced: true },
      ],
      containsDebug: false,
      replaceFileNames: true,
      empty: false,
    });
    renderDialog();
    fireEvent.click(screen.getByText('Ja, Logdatei anhängen'));
    await waitFor(() => expect(diagnosticsApi.previewLogExport).toHaveBeenCalledWith(undefined));

    const replaced = await screen.findByText('C:\\Users\\Anna');
    expect(replaced.tagName).toBe('MARK');
    expect(screen.getByRole('checkbox')).toBeChecked();
  });

  it('reloads the preview without file-name replacement when the checkbox is toggled', async () => {
    renderDialog();
    fireEvent.click(screen.getByText('Ja, Logdatei anhängen'));
    await waitFor(() => expect(diagnosticsApi.previewLogExport).toHaveBeenCalledTimes(1));

    vi.mocked(diagnosticsApi.previewLogExport).mockResolvedValue({
      segments: [{ text: 'plain', replaced: false }],
      containsDebug: false,
      replaceFileNames: false,
      empty: false,
    });
    fireEvent.click(screen.getByRole('checkbox'));
    await waitFor(() => expect(diagnosticsApi.previewLogExport).toHaveBeenCalledWith(false));
  });

  it('saves the log, opens the form and keeps the dialog open', async () => {
    renderDialog();
    fireEvent.click(screen.getByText('Ja, Logdatei anhängen'));
    await waitFor(() => expect(screen.getByRole('checkbox')).toBeInTheDocument());

    fireEvent.click(screen.getByText('Speichern und Formular öffnen'));
    await waitFor(() => expect(diagnosticsApi.saveLogExport).toHaveBeenCalled());
    await waitFor(() => expect(diagnosticsApi.openBugReportForm).toHaveBeenCalledWith('de', true));

    expect(screen.getByText(/3mf-katalog-log-2026-09-26\.txt/)).toBeInTheDocument();
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByText('Schließen')).toBeInTheDocument();
  });

  it('shows the empty-log hint instead of a preview', async () => {
    vi.mocked(diagnosticsApi.previewLogExport).mockResolvedValue({
      segments: [],
      containsDebug: false,
      replaceFileNames: false,
      empty: true,
    });
    renderDialog();
    fireEvent.click(screen.getByText('Ja, Logdatei anhängen'));
    await waitFor(() => expect(screen.getByText('Das Protokoll ist noch leer.')).toBeInTheDocument());
  });

  it('closes on Escape', () => {
    const onClose = renderDialog();
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });
});
