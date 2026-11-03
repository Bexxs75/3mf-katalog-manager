import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { de } from '../i18n/de';
import { UpgradeNoticeDialog } from './UpgradeNoticeDialog';
import { normalizeUpgradeNotice } from '../lib/api/upgradeNotice';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const UPDATE_COPY = 'Katalog-Sicherung_2026-11-03_1430_vor-Update_0.17.1_auf_0.18.0.db';
const SCHEMA_COPY = 'Katalog-Sicherung_2026-11-03_1432_vor-Schema-Migration_52_auf_54.db';

const base = {
  fromVersion: '0.17.1',
  fromVersionDerived: false,
  toVersion: '0.18.0',
  fromSchema: 52,
  toSchema: 54,
  backup: { fileName: UPDATE_COPY, path: `/data/update-backups/${UPDATE_COPY}`, sizeBytes: 18_400_000, modified: '2026-11-03T14:30:00+01:00' },
  backupFailed: false,
  folder: '/data/update-backups',
  catalogFile: '/data/catalog.db',
  previousReleaseUrl: 'https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.17.1',
  container: false,
};

function answer(notice: unknown, others: Record<string, unknown> = {}) {
  vi.mocked(invoke).mockImplementation(async (command: string) => {
    if (command === 'get_upgrade_notice') return notice;
    if (command in others) { const v = others[command]; if (v && typeof v === 'object' && 'message' in v) throw v; return v; }
    return undefined;
  });
}
const calls = (command: string) => vi.mocked(invoke).mock.calls.filter(([c]) => c === command);
const renderDialog = (props: { wait?: boolean; onSettled?: () => void } = {}) =>
  render(<LanguageProvider><UpgradeNoticeDialog {...props} /></LanguageProvider>);

beforeEach(() => { localStorage.setItem('3mf-katalog-language', 'de'); vi.mocked(invoke).mockReset(); });

describe('UpgradeNoticeDialog', () => {
  it('after an update names versions, the copy, the folder and the steps back', async () => {
    answer(base);
    renderDialog();
    const dialog = await screen.findByRole('dialog', { name: de.rwTitle });
    expect(dialog).toHaveTextContent(UPDATE_COPY);
    expect(dialog).toHaveTextContent('0.17.1');
    expect(dialog).toHaveTextContent('/data/update-backups');
    expect(dialog).toHaveTextContent(de.rwStepsTitle);
    expect(dialog).toHaveTextContent(de.rwConsequenceLost);
    // The copy step names both files in full.
    expect(dialog).toHaveTextContent(`/data/update-backups/${UPDATE_COPY}`);
    expect(dialog).toHaveTextContent('/data/catalog.db');
    expect(screen.getByRole('button', { name: de.rwOpenFolder })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: de.rwDownloadPrevious })).toBeInTheDocument();
  });

  it('puts keyboard focus on "Understood" and keeps Tab inside the dialog', async () => {
    answer(base);
    renderDialog();
    const ok = await screen.findByRole('button', { name: de.rwUnderstood });
    await waitFor(() => expect(ok).toHaveFocus());
    // Tab from the last control wraps to the first one, Shift+Tab from the first to the last.
    const buttons = screen.getAllByRole('button');
    const first = buttons[0];
    const last = buttons[buttons.length - 1];
    last.focus();
    fireEvent.keyDown(last, { key: 'Tab' });
    expect(first).toHaveFocus();
    fireEvent.keyDown(first, { key: 'Tab', shiftKey: true });
    expect(last).toHaveFocus();
  });

  it('treats Escape like "Understood": remembers it and closes', async () => {
    answer(base);
    const onSettled = vi.fn();
    renderDialog({ onSettled });
    await screen.findByRole('dialog');
    fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(calls('dismiss_upgrade_notice')).toHaveLength(1);
    expect(onSettled).toHaveBeenCalledTimes(1);
  });

  it('"Understood" remembers the answer once and closes', async () => {
    answer(base);
    renderDialog();
    fireEvent.click(await screen.findByRole('button', { name: de.rwUnderstood }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(calls('dismiss_upgrade_notice')).toHaveLength(1);
  });

  it('a manual install shows a derived previous version and says so', async () => {
    answer({ ...base, fromVersionDerived: true, backup: { ...base.backup, fileName: SCHEMA_COPY, path: `/data/update-backups/${SCHEMA_COPY}` } });
    renderDialog();
    const dialog = await screen.findByRole('dialog');
    expect(dialog).toHaveTextContent('0.18.0 (vorher: 0.17.1, aus dem Datenbank-Schema abgeleitet)');
    expect(dialog).toHaveTextContent(SCHEMA_COPY);
  });

  it('a manual install with an unknown previous version offers no download and does not guess', async () => {
    answer({ ...base, fromVersion: null, previousReleaseUrl: null });
    renderDialog();
    const dialog = await screen.findByRole('dialog');
    expect(dialog).toHaveTextContent(de.rwStepInstallUnknown);
    expect(dialog).toHaveTextContent('0.18.0 (vorherige Version unbekannt)');
    expect(screen.queryByRole('button', { name: de.rwDownloadPrevious })).not.toBeInTheDocument();
  });

  it('a failed backup shows the warning and no way back', async () => {
    answer({ ...base, backup: null, backupFailed: true, previousReleaseUrl: null });
    renderDialog();
    await screen.findByRole('dialog', { name: de.rwTitleFailed });
    expect(screen.getByRole('alert')).toHaveTextContent(de.migrationBackupWarning);
    expect(screen.queryByRole('heading', { name: de.rwStepsTitle })).not.toBeInTheDocument();
    expect(screen.queryByRole('list')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: de.rwOpenFolder })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: de.rwDownloadPrevious })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: de.rwUnderstood })).toBeInTheDocument();
  });

  it('in a container shows where the copy is but offers no folder or link button and invents no steps', async () => {
    answer({ ...base, container: true, previousReleaseUrl: null });
    renderDialog();
    const dialog = await screen.findByRole('dialog');
    expect(dialog).toHaveTextContent('/data/update-backups (im Container)');
    expect(dialog).toHaveTextContent(UPDATE_COPY);
    expect(dialog).toHaveTextContent(de.rwContainerNote);
    expect(screen.queryByRole('heading', { name: de.rwStepsTitle })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: de.rwOpenFolder })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: de.rwDownloadPrevious })).not.toBeInTheDocument();
  });

  it('opens the backup folder through the fixed command, without sending a path', async () => {
    answer(base);
    renderDialog();
    fireEvent.click(await screen.findByRole('button', { name: de.rwOpenFolder }));
    await waitFor(() => expect(calls('open_update_backup_folder')).toHaveLength(1));
    expect(calls('open_update_backup_folder')[0]).toEqual(['open_update_backup_folder']);
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });

  it('explains a missing backup folder in words and keeps the dialog open', async () => {
    answer(base, { open_update_backup_folder: { message: 'updateBackupFolderMissing', expected: true, params: [] } });
    renderDialog();
    fireEvent.click(await screen.findByRole('button', { name: de.rwOpenFolder }));
    expect(await screen.findByText(de.errUpdateBackupFolderMissing)).toBeInTheDocument();
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(calls('dismiss_upgrade_notice')).toHaveLength(0);
  });

  it('opens the release page of the previous version', async () => {
    answer(base);
    renderDialog();
    fireEvent.click(await screen.findByRole('button', { name: de.rwDownloadPrevious }));
    await waitFor(() => expect(calls('open_release_url')).toHaveLength(1));
    expect(calls('open_release_url')[0]).toEqual(['open_release_url', { url: base.previousReleaseUrl }]);
  });

  it('shows nothing, and says it is settled, when the backend has no notice', async () => {
    answer(null);
    const onSettled = vi.fn();
    renderDialog({ onSettled });
    await waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('does not ask before the setup dialog is gone, then asks once', async () => {
    answer(base);
    const { rerender } = renderDialog({ wait: true });
    await act(async () => { await Promise.resolve(); });
    expect(calls('get_upgrade_notice')).toHaveLength(0);
    rerender(<LanguageProvider><UpgradeNoticeDialog wait={false} /></LanguageProvider>);
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
    expect(calls('get_upgrade_notice')).toHaveLength(1);
  });

  it('a failing lookup never blocks the start', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('boom'));
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const onSettled = vi.fn();
    renderDialog({ onSettled });
    await waitFor(() => expect(onSettled).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    warn.mockRestore();
  });
});

describe('normalizeUpgradeNotice', () => {
  it('accepts the backend shape', () => {
    expect(normalizeUpgradeNotice(base)?.backup?.fileName).toBe(UPDATE_COPY);
  });
  it('ignores malformed or empty answers', () => {
    for (const bad of [null, undefined, 'x', {}, { ...base, toVersion: '' }, { ...base, backup: null, backupFailed: false }, { ...base, backup: { fileName: 1 } }]) {
      expect(normalizeUpgradeNotice(bad)).toBeNull();
    }
  });
});
