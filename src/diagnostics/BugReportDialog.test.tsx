import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { BugReportDialog } from './BugReportDialog';
import * as diagnosticsApi from '../lib/api/diagnostics';
import type { LogPreview } from '../lib/api/diagnostics';

vi.mock('../lib/api/diagnostics');

const INFO = { version: '0.15.0', os: 'linux' as const };

function renderDialog(onClose: () => void = vi.fn()) {
  const { unmount } = render(
    <LanguageProvider>
      <BugReportDialog onClose={onClose} />
    </LanguageProvider>,
  );
  return { onClose, unmount };
}

function openButton() {
  return screen.getByText('Formular öffnen').closest('button')!;
}

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(diagnosticsApi.getBugReportInfo).mockResolvedValue(INFO);
  vi.mocked(diagnosticsApi.previewLogExport).mockResolvedValue({
    id: 1,
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
      id: 1,
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
      id: 2,
      segments: [{ text: 'plain', replaced: false }],
      containsDebug: false,
      replaceFileNames: false,
      empty: false,
    });
    fireEvent.click(screen.getByRole('checkbox'));
    await waitFor(() => expect(diagnosticsApi.previewLogExport).toHaveBeenCalledWith(false));
  });

  it('ignores a stale preview response that resolves after a newer one', async () => {
    renderDialog();
    fireEvent.click(screen.getByText('Ja, Logdatei anhängen'));
    await waitFor(() => expect(screen.getByRole('checkbox')).toBeInTheDocument());

    let resolveStale!: (v: LogPreview) => void;
    let resolveFresh!: (v: LogPreview) => void;
    vi.mocked(diagnosticsApi.previewLogExport)
      .mockImplementationOnce(() => new Promise((r) => { resolveStale = r; }))
      .mockImplementationOnce(() => new Promise((r) => { resolveFresh = r; }));

    // Two rapid toggles: the first (older) request resolves last.
    fireEvent.click(screen.getByRole('checkbox'));
    fireEvent.click(screen.getByRole('checkbox'));
    await waitFor(() => expect(diagnosticsApi.previewLogExport).toHaveBeenCalledTimes(3));

    await act(async () => {
      resolveFresh({ id: 3, segments: [{ text: 'fresh', replaced: false }], containsDebug: false, replaceFileNames: true, empty: false });
    });
    expect(screen.getByText('fresh')).toBeInTheDocument();

    await act(async () => {
      resolveStale({ id: 2, segments: [{ text: 'stale', replaced: false }], containsDebug: false, replaceFileNames: false, empty: false });
    });
    expect(screen.queryByText('stale')).not.toBeInTheDocument();
    expect(screen.getByText('fresh')).toBeInTheDocument();
  });

  it('saves the log, opens the form and keeps the dialog open', async () => {
    renderDialog();
    fireEvent.click(screen.getByText('Ja, Logdatei anhängen'));
    await waitFor(() => expect(screen.getByRole('checkbox')).toBeInTheDocument());

    fireEvent.click(screen.getByText('Speichern und Formular öffnen'));
    await waitFor(() => expect(diagnosticsApi.saveLogExport).toHaveBeenCalledWith(1));
    await waitFor(() => expect(diagnosticsApi.openBugReportForm).toHaveBeenCalledWith('de', true));

    expect(screen.getByText(/3mf-katalog-log-2026-09-26\.txt/)).toBeInTheDocument();
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByText('Schließen')).toBeInTheDocument();
  });

  it('ignores a second click on the main button while a submit is already in flight', async () => {
    let resolveForm!: () => void;
    vi.mocked(diagnosticsApi.openBugReportForm).mockImplementation(
      () => new Promise((resolve) => { resolveForm = () => resolve(undefined); }),
    );
    renderDialog();
    fireEvent.click(screen.getByText('Nein, ohne Logdatei melden'));
    const button = openButton();

    fireEvent.click(button);
    expect(button).toBeDisabled();
    fireEvent.click(button);

    expect(diagnosticsApi.openBugReportForm).toHaveBeenCalledTimes(1);
    await act(async () => resolveForm());
  });

  it('shows the empty-log hint instead of a preview', async () => {
    vi.mocked(diagnosticsApi.previewLogExport).mockResolvedValue({
      id: 1,
      segments: [],
      containsDebug: false,
      replaceFileNames: false,
      empty: true,
    });
    renderDialog();
    fireEvent.click(screen.getByText('Ja, Logdatei anhängen'));
    await waitFor(() => expect(screen.getByText('Das Protokoll ist noch leer.')).toBeInTheDocument());
  });

  it('closes on Escape once the dialog has taken focus, even from outside it', async () => {
    const outside = document.createElement('button');
    document.body.appendChild(outside);
    outside.focus();
    expect(document.activeElement).toBe(outside);

    const { onClose } = renderDialog();
    // Opening moves focus to the first useful control in the dialog.
    await waitFor(() => expect(screen.getByRole('dialog')).toContainElement(document.activeElement as HTMLElement));

    fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
    outside.remove();
  });

  it('ignores Escape while a save is in flight', async () => {
    let resolveSave!: (path: string) => void;
    vi.mocked(diagnosticsApi.saveLogExport).mockImplementation(
      () => new Promise((resolve) => { resolveSave = resolve; }),
    );
    const { onClose } = renderDialog();
    fireEvent.click(screen.getByText('Ja, Logdatei anhängen'));
    await waitFor(() => expect(screen.getByRole('checkbox')).toBeInTheDocument());

    fireEvent.click(screen.getByText('Speichern und Formular öffnen'));
    await waitFor(() => expect(diagnosticsApi.saveLogExport).toHaveBeenCalled());

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog')).toBeInTheDocument();

    await act(async () => resolveSave('/home/user/Downloads/x.txt'));
  });

  it('offers a retry button to open the form when the save succeeded but opening it failed', async () => {
    vi.mocked(diagnosticsApi.openBugReportForm).mockRejectedValueOnce({ message: 'Browser konnte nicht gestartet werden', expected: false });
    renderDialog();
    fireEvent.click(screen.getByText('Ja, Logdatei anhängen'));
    await waitFor(() => expect(screen.getByRole('checkbox')).toBeInTheDocument());

    fireEvent.click(screen.getByText('Speichern und Formular öffnen'));
    await waitFor(() => expect(diagnosticsApi.saveLogExport).toHaveBeenCalledWith(1));
    await screen.findByText('Browser konnte nicht gestartet werden');

    // The log was saved, so the main "save and open" button is gone - but the
    // user must still be able to retry opening the form.
    expect(screen.queryByText('Speichern und Formular öffnen')).not.toBeInTheDocument();
    const retryButton = screen.getByText('Formular öffnen').closest('button')!;

    vi.mocked(diagnosticsApi.openBugReportForm).mockResolvedValueOnce(undefined);
    fireEvent.click(retryButton);
    await waitFor(() => expect(diagnosticsApi.openBugReportForm).toHaveBeenCalledWith('de', true));
    expect(diagnosticsApi.openBugReportForm).toHaveBeenCalledTimes(2);
    await waitFor(() => expect(screen.queryByText('Browser konnte nicht gestartet werden')).not.toBeInTheDocument());
  });

  it('still opens the form after a successful save even if the dialog unmounted meanwhile', async () => {
    let resolveSave!: (path: string) => void;
    vi.mocked(diagnosticsApi.saveLogExport).mockImplementation(
      () => new Promise((resolve) => { resolveSave = resolve; }),
    );
    const { unmount } = renderDialog();
    fireEvent.click(screen.getByText('Ja, Logdatei anhängen'));
    await waitFor(() => expect(screen.getByRole('checkbox')).toBeInTheDocument());

    fireEvent.click(screen.getByText('Speichern und Formular öffnen'));
    await waitFor(() => expect(diagnosticsApi.saveLogExport).toHaveBeenCalled());

    // The dialog goes away (e.g. the whole app unmounts) before the save
    // resolves; the browser tab must still open once it does.
    unmount();
    await act(async () => resolveSave('/home/user/Downloads/x.txt'));

    expect(diagnosticsApi.openBugReportForm).toHaveBeenCalledWith('de', true);
  });
});
