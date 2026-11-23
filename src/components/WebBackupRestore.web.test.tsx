import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { LanguageProvider } from '../i18n/LanguageContext';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';

const api = vi.hoisted(() => ({ uploadBackup: vi.fn(), discardBackup: vi.fn(), restoreBackup: vi.fn(), waitForServer: vi.fn() }));
vi.mock('../lib/api/backupRestore', async () => ({ ...await vi.importActual<typeof import('../lib/api/backupRestore')>('../lib/api/backupRestore'), ...api }));
const session = vi.hoisted(() => ({ can: true }));
vi.mock('../lib/webSession', async () => ({ ...await vi.importActual<typeof import('../lib/webSession')>('../lib/webSession'), useCanEditMetadata: () => session.can }));

import { WebBackupRestore } from './WebBackupRestore';
import { BACKUP_MAX_BYTES } from '../lib/api/backupRestore';

const report = {
  schema_version: 62, migration_needed: true, model_count: 4218, folder_count: 312,
  path_warnings: Array.from({ length: 7 }, (_, i) => ({ table: 'files', column: 'path', row_id: i, display_path: `Vase_${i}.3mf` })),
  unreachable_source_folders: ['/mnt/nas/Alt-2023'],
};
const preview = { token: 'a'.repeat(64), report, settingsIgnored: true };
const zip = (name = 'katalog.zip', size = 10) => { const file = new File(['x'], name, { type: 'application/zip' }); Object.defineProperty(file, 'size', { value: size, configurable: true }); return file; };

function show() {
  return render(<LanguageProvider><WebBackupRestore /></LanguageProvider>);
}
const openDialog = () => fireEvent.click(screen.getByRole('button', { name: de.bkrOpen }));
const dialog = () => screen.getByRole('dialog', { name: de.bkrTitle });
function pickFile(file: File) {
  const input = document.querySelector('input[type="file"]') as HTMLInputElement;
  fireEvent.change(input, { target: { files: [file] } });
}
const typePassword = (value: string) => fireEvent.change(screen.getByLabelText(de.bkrPwLabel), { target: { value } });
async function toReport() {
  openDialog(); pickFile(zip());
  fireEvent.click(screen.getByRole('button', { name: de.bkrNext }));
  typePassword('geheim-1');
  fireEvent.click(screen.getByRole('button', { name: de.bkrCheck }));
  await screen.findByRole('heading', { name: de.bkrReportTitle });
}
async function toConfirm() {
  await toReport();
  fireEvent.click(screen.getByRole('button', { name: de.bkrToConfirm }));
  await screen.findByRole('heading', { name: de.bkrConfirmTitle });
}
const submitReplace = () => {
  fireEvent.click(screen.getByLabelText(de.bkrAck));
  typePassword('geheim-2');
  fireEvent.click(screen.getByRole('button', { name: de.bkrReplace }));
};

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  session.can = true;
  api.uploadBackup.mockResolvedValue(preview);
  api.discardBackup.mockResolvedValue(undefined);
  api.restoreBackup.mockResolvedValue({ restored: true, restart: true });
  api.waitForServer.mockResolvedValue(true);
});
afterEach(() => { cleanup(); vi.clearAllMocks(); });

describe('entry point', () => {
  it('opens the dialog from a real button and gives the focus back when it closes', () => {
    show();
    const button = screen.getByRole('button', { name: de.bkrOpen });
    button.focus();
    openDialog();
    expect(dialog()).toHaveAttribute('aria-modal', 'true');
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(button).toHaveFocus();
  });

  it('shows the area disabled, with its reason, for a login that may only look', () => {
    session.can = false;
    show();
    const button = screen.getByRole('button', { name: de.bkrOpen });
    expect(button).toHaveAttribute('aria-disabled', 'true');
    expect(button).toHaveAccessibleDescription(de.bkrReadOnly);
    fireEvent.click(button);
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});

describe('step 1: file', () => {
  it('starts on the step heading with the step shown as text and as a list', () => {
    show(); openDialog();
    expect(screen.getByRole('heading', { name: de.bkrFileTitle })).toHaveFocus();
    expect(within(dialog()).getByText(/Schritt 1 von 6/)).toBeInTheDocument();
    expect(within(dialog()).getByRole('list', { name: de.bkrStepsLabel }).querySelector('[aria-current="step"]')).not.toBeNull();
    expect(screen.getByRole('button', { name: de.bkrNext })).toBeDisabled();
  });

  it('names a file that is not a ZIP, is empty or is too large, and sends nothing', () => {
    show(); openDialog();
    pickFile(zip('bild.png'));
    expect(screen.getByRole('alert')).toHaveTextContent('„bild.png“ ist keine ZIP-Datei');
    expect(screen.getByRole('button', { name: de.bkrNext })).toBeDisabled();
    pickFile(zip('leer.zip', 0));
    expect(screen.getByRole('alert')).toHaveTextContent('„leer.zip“ ist leer');
    pickFile(zip('gross.zip', BACKUP_MAX_BYTES + 1));
    expect(screen.getByRole('alert')).toHaveTextContent('größer als die Grenze von 258');
    expect(screen.getByRole('button', { name: de.bkrNext })).toBeDisabled();
    expect(api.uploadBackup).not.toHaveBeenCalled();
  });

  it('accepts a ZIP at the limit and moves on with the next button', () => {
    show(); openDialog();
    pickFile(zip('katalog.zip', BACKUP_MAX_BYTES));
    expect(screen.queryByRole('alert')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: de.bkrNext }));
    expect(screen.getByRole('heading', { name: de.bkrPwTitle })).toHaveFocus();
  });
});

describe('step 2: password and check', () => {
  it('asks for the password first and sends nothing without it', () => {
    show(); openDialog(); pickFile(zip());
    fireEvent.click(screen.getByRole('button', { name: de.bkrNext }));
    fireEvent.click(screen.getByRole('button', { name: de.bkrCheck }));
    expect(screen.getByRole('alert')).toHaveTextContent(de.bkrPwEmpty);
    expect(screen.getByLabelText(de.bkrPwLabel)).toHaveAttribute('aria-invalid', 'true');
    expect(screen.getByLabelText(de.bkrPwLabel)).toHaveFocus();
    expect(api.uploadBackup).not.toHaveBeenCalled();
  });

  it('uploads with the password, then empties the field so the secret is not left in the page', async () => {
    let finish: (value: unknown) => void = () => {};
    api.uploadBackup.mockImplementation((_f: File, _p: string, onProgress: (n: number) => void) => { onProgress(40); return new Promise((resolve) => { finish = resolve; }); });
    show(); openDialog(); pickFile(zip());
    fireEvent.click(screen.getByRole('button', { name: de.bkrNext }));
    typePassword('geheim-1');
    fireEvent.click(screen.getByRole('button', { name: de.bkrCheck }));
    expect(api.uploadBackup.mock.calls[0][1]).toBe('geheim-1');
    expect(screen.getByRole('progressbar', { name: de.bkrUploadLabel })).toHaveValue(40);
    expect(screen.getByLabelText(de.bkrPwLabel)).toHaveValue('');
    expect(document.body.innerHTML).not.toContain('geheim-1');
    await act(async () => { finish(preview); });
    await screen.findByRole('heading', { name: de.bkrReportTitle });
    expect(document.body.innerHTML).not.toContain('geheim-1');
  });

  it('shows a wrong password beside the field and keeps the focus there', async () => {
    api.uploadBackup.mockRejectedValue({ message: 'password_invalid', expected: true, status: 403 });
    show(); openDialog(); pickFile(zip());
    fireEvent.click(screen.getByRole('button', { name: de.bkrNext }));
    typePassword('falsch');
    fireEvent.click(screen.getByRole('button', { name: de.bkrCheck }));
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent(de.bkrErrPassword);
    const field = screen.getByLabelText(de.bkrPwLabel);
    expect(field).toHaveAttribute('aria-describedby', alert.id);
    await waitFor(() => expect(field).toHaveFocus());
  });

  it.each([
    ['backupInvalidDatabase', de.errBackupInvalidDatabase],
    ['backupNotZip', de.errBackupNotZip],
    ['maintenance', de.errMaintenance],
    ['webTooManyRequests', de.errWebTooManyRequests],
    ['backupUploadTooLarge', de.bkrErrTooLarge],
    ['backupConnectionLost', de.bkrErrConnection],
  ])('turns the answer %s into its translated text', async (message, text) => {
    api.uploadBackup.mockRejectedValue({ message, expected: true, status: 400 });
    show(); openDialog(); pickFile(zip());
    fireEvent.click(screen.getByRole('button', { name: de.bkrNext }));
    typePassword('x');
    fireEvent.click(screen.getByRole('button', { name: de.bkrCheck }));
    expect(await screen.findByRole('alert')).toHaveTextContent(text);
  });
});

describe('step 3: report', () => {
  it('shows what is inside, the migration, the warnings and the ignored settings, with the heading focused', async () => {
    show(); await toReport();
    expect(screen.getByRole('heading', { name: de.bkrReportTitle })).toHaveFocus();
    expect(dialog()).toHaveTextContent('Schema 62: Beim Einspielen wird die Sicherung automatisch');
    expect(dialog()).toHaveTextContent('4.218');
    expect(dialog()).toHaveTextContent('312');
    expect(dialog()).toHaveTextContent('Nicht erreichbare Quellordner: 1');
    expect(dialog()).toHaveTextContent('Alt-2023');
    expect(dialog()).not.toHaveTextContent('/mnt/nas');
    expect(dialog()).toHaveTextContent(de.bkrSettingsIgnored);
  });

  it('says so plainly when no migration is needed', async () => {
    api.uploadBackup.mockResolvedValue({ ...preview, report: { ...report, schema_version: 66, migration_needed: false, path_warnings: [], unreachable_source_folders: [] } });
    show(); await toReport();
    expect(dialog()).toHaveTextContent('Schema 66, keine Migration nötig.');
    expect(screen.queryByRole('button', { name: /Pfade außerhalb/ })).toBeNull();
  });

  it('opens the path warnings on demand, shows five and counts the rest', async () => {
    show(); await toReport();
    const toggle = screen.getByRole('button', { name: /Pfade außerhalb der freigegebenen Katalogordner \(7\)/ });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    toggle.focus();
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(document.getElementById(toggle.getAttribute('aria-controls')!)).toHaveTextContent('Vase_4.3mf');
    expect(dialog()).not.toHaveTextContent('Vase_5.3mf');
    expect(dialog()).toHaveTextContent('… und 2 weitere');
    expect(toggle).toHaveFocus();
  });

  it('discards the checked backup on the server and starts over', async () => {
    show(); await toReport();
    fireEvent.click(screen.getByRole('button', { name: de.bkrDiscard }));
    expect(api.discardBackup).toHaveBeenCalledWith(preview.token);
    expect(screen.getByRole('heading', { name: de.bkrFileTitle })).toHaveFocus();
    expect(screen.getByRole('button', { name: de.bkrNext })).toBeDisabled();
  });

  it('discards the checked backup when the dialog is closed with Escape', async () => {
    show(); await toReport();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(api.discardBackup).toHaveBeenCalledWith(preview.token);
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});

describe('step 4: confirm and restore', () => {
  it('warns clearly and does not put the focus on the destructive button', async () => {
    show(); await toConfirm();
    expect(screen.getByRole('heading', { name: de.bkrConfirmTitle })).toHaveFocus();
    for (const key of ['bkrWarnHeading', 'bkrWarn1', 'bkrWarn2', 'bkrWarn3', 'bkrWarn4'] as const) expect(dialog()).toHaveTextContent(de[key]);
    expect(screen.getByRole('button', { name: de.bkrReplace })).not.toHaveFocus();
    expect(screen.getByRole('button', { name: de.bkrReplace })).toBeDisabled();
  });

  it('replaces only after the tick and a freshly typed password', async () => {
    show(); await toConfirm();
    const replace = screen.getByRole('button', { name: de.bkrReplace });
    fireEvent.click(screen.getByLabelText(de.bkrAck));
    expect(replace).toBeDisabled();
    fireEvent.submit(replace.closest('form')!);
    expect(api.restoreBackup).not.toHaveBeenCalled();
    typePassword('geheim-2');
    expect(replace).toBeEnabled();
    fireEvent.click(replace);
    expect(api.restoreBackup).toHaveBeenCalledWith(preview.token, 'geheim-2');
  });

  it('does not restore on Enter in the password field while the tick is missing', async () => {
    show(); await toConfirm();
    typePassword('geheim-2');
    fireEvent.submit(screen.getByLabelText(de.bkrPwLabel).closest('form')!);
    expect(api.restoreBackup).not.toHaveBeenCalled();
  });

  it('blocks Escape and the close button while the server replaces the catalog', async () => {
    api.restoreBackup.mockReturnValue(new Promise(() => {}));
    show(); await toConfirm();
    submitReplace();
    expect(await screen.findByRole('heading', { name: de.bkrWorkTitle })).toBeInTheDocument();
    expect(screen.getByRole('progressbar', { name: de.bkrProgressLabel })).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: de.bkrClose })).toBeDisabled();
    expect(api.discardBackup).not.toHaveBeenCalled();
    expect(document.body.innerHTML).not.toContain('geheim-2');
  });

  it('goes back to the confirmation with the field marked when the password is rejected', async () => {
    api.restoreBackup.mockRejectedValue({ message: 'password_invalid', expected: true, status: 403 });
    show(); await toConfirm();
    submitReplace();
    expect(await screen.findByRole('alert')).toHaveTextContent(de.bkrErrPassword);
    expect(screen.getByRole('heading', { name: de.bkrConfirmTitle })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByLabelText(de.bkrPwLabel)).toHaveFocus());
    expect(screen.getByLabelText(de.bkrPwLabel)).toHaveValue('');
  });

  it('sends the user back to the start when the checked backup has expired', async () => {
    api.restoreBackup.mockRejectedValue({ message: 'webNotFound', expected: true, status: 404 });
    show(); await toConfirm();
    submitReplace();
    expect(await screen.findByRole('heading', { name: de.bkrFileTitle })).toBeInTheDocument();
    expect(screen.getByRole('alert')).toHaveTextContent(de.bkrErrTokenGone);
  });
});

describe('result', () => {
  it('after success waits for the server, then offers the sign-in with the focus on it', async () => {
    let up: (value: boolean) => void = () => {};
    api.waitForServer.mockReturnValue(new Promise((resolve) => { up = resolve; }));
    show(); await toConfirm();
    submitReplace();
    expect(await screen.findByRole('heading', { name: de.bkrDoneTitle })).toBeInTheDocument();
    expect(dialog()).toHaveTextContent(de.bkrDoneText);
    expect(dialog()).toHaveTextContent(de.bkrWaiting);
    expect(screen.getByRole('button', { name: de.bkrToLogin })).toBeDisabled();
    await act(async () => { up(true); });
    expect(dialog()).toHaveTextContent(de.bkrServerUp);
    const login = screen.getByRole('button', { name: de.bkrToLogin });
    expect(login).toBeEnabled();
    expect(login).toHaveFocus();
    // The used token is gone; closing the dialog must not ask the server to discard it.
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(api.discardBackup).not.toHaveBeenCalled();
  });

  it('gives up waiting with a clear note about a missing restart policy, and can check again', async () => {
    api.waitForServer.mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    show(); await toConfirm();
    submitReplace();
    await waitFor(() => expect(dialog()).toHaveTextContent(de.bkrServerSlow));
    expect(dialog()).toHaveTextContent('restart: unless-stopped');
    fireEvent.click(screen.getByRole('button', { name: de.bkrCheckAgain }));
    await waitFor(() => expect(dialog()).toHaveTextContent(de.bkrServerUp));
    expect(api.waitForServer).toHaveBeenCalledTimes(2);
  });

  it('does not claim success when the connection broke before the answer', async () => {
    api.restoreBackup.mockRejectedValue({ message: 'backupConnectionLost', expected: true, status: 0 });
    show(); await toConfirm();
    submitReplace();
    expect(await screen.findByRole('heading', { name: de.bkrDoneTitle })).toBeInTheDocument();
    expect(dialog()).toHaveTextContent(de.bkrUnknownOutcome);
    expect(dialog()).not.toHaveTextContent(de.bkrDoneText);
    await waitFor(() => expect(api.waitForServer).toHaveBeenCalled());
  });

  it('treats a proxy timeout like a lost answer', async () => {
    api.restoreBackup.mockRejectedValue({ message: 'webServerInternal', expected: true, status: 504 });
    show(); await toConfirm();
    submitReplace();
    expect(await screen.findByText(de.bkrUnknownOutcome)).toBeInTheDocument();
  });

  it.each([
    ['restoreBackupInvalid', de.errRestoreBackupInvalid, false],
    ['restoreFailed', de.errRestoreFailed, false],
    ['restoreRecoveryFailed', de.errRestoreRecoveryFailed, false],
    ['restoreDrainTimeout', de.errRestoreDrainTimeout, true],
    ['restoreJournalPending', de.errRestoreJournalPending, true],
    ['restoreUploadsPending', de.errRestoreUploadsPending, true],
    ['maintenance', de.errMaintenance, true],
  ])('shows %s translated, and offers a retry only when the backup is still waiting', async (message, text, retry) => {
    api.restoreBackup.mockRejectedValue({ message, expected: true, status: 409 });
    show(); await toConfirm();
    submitReplace();
    expect(await screen.findByRole('heading', { name: de.bkrFailTitle })).toHaveFocus();
    expect(screen.getByRole('alert')).toHaveTextContent(text);
    expect(!!screen.queryByRole('button', { name: de.bkrTryAgain })).toBe(retry);
  });

  it('retries from the confirmation with the same checked backup', async () => {
    api.restoreBackup.mockRejectedValueOnce({ message: 'restoreUploadsPending', expected: true, status: 409 });
    show(); await toConfirm();
    submitReplace();
    fireEvent.click(await screen.findByRole('button', { name: de.bkrTryAgain }));
    expect(screen.getByRole('heading', { name: de.bkrConfirmTitle })).toBeInTheDocument();
    submitReplace();
    await waitFor(() => expect(api.restoreBackup).toHaveBeenCalledTimes(2));
    expect(api.restoreBackup.mock.calls[1][0]).toBe(preview.token);
  });

  it('removes a backup that is still waiting when another file is chosen after a failure', async () => {
    api.restoreBackup.mockRejectedValue({ message: 'restoreDrainTimeout', expected: true, status: 503 });
    show(); await toConfirm();
    submitReplace();
    fireEvent.click(await screen.findByRole('button', { name: de.bkrFailChooseOther }));
    expect(api.discardBackup).toHaveBeenCalledWith(preview.token);
    expect(screen.getByRole('heading', { name: de.bkrFileTitle })).toBeInTheDocument();
  });
});

describe('texts', () => {
  it('has every dialog text in all four languages', () => {
    const keys = Object.keys(de).filter((key) => key.startsWith('bkr'));
    expect(keys.length).toBeGreaterThan(60);
    for (const dictionary of [en, es, fr]) for (const key of keys) expect((dictionary as unknown as Record<string, string>)[key]).toBeTruthy();
  });
});
