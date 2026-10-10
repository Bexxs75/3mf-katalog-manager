import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as diagnosticsApi from './diagnostics';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

describe('diagnostics api', () => {
  it('getBugReportInfo', async () => {
    const result = { version: '0.15.0', os: 'linux' as const };
    vi.mocked(invoke).mockResolvedValue(result);
    const actual = await diagnosticsApi.getBugReportInfo();
    expect(invoke).toHaveBeenCalledWith('get_bug_report_info');
    expect(actual).toEqual(result);
  });

  it('previewLogExport without argument sends null', async () => {
    const result = { id: 1, segments: [], containsDebug: false, replaceFileNames: true, empty: true };
    vi.mocked(invoke).mockResolvedValue(result);
    const actual = await diagnosticsApi.previewLogExport();
    expect(invoke).toHaveBeenCalledWith('preview_log_export', { replaceFileNames: null });
    expect(actual).toEqual(result);
  });

  it('previewLogExport forwards an explicit replaceFileNames value', async () => {
    vi.mocked(invoke).mockResolvedValue({ id: 2, segments: [], containsDebug: false, replaceFileNames: false, empty: false });
    await diagnosticsApi.previewLogExport(false);
    expect(invoke).toHaveBeenCalledWith('preview_log_export', { replaceFileNames: false });
  });

  it('saveLogExport passes the previewed id', async () => {
    vi.mocked(invoke).mockResolvedValue('/home/user/Downloads/3mf-katalog-log-2026-09-26.txt');
    const actual = await diagnosticsApi.saveLogExport(3);
    expect(invoke).toHaveBeenCalledWith('save_log_export', { id: 3 });
    expect(actual).toBe('/home/user/Downloads/3mf-katalog-log-2026-09-26.txt');
  });

  it('openLogFolder', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await diagnosticsApi.openLogFolder();
    expect(invoke).toHaveBeenCalledWith('open_log_folder');
  });

  it('openDataFolder', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await diagnosticsApi.openDataFolder();
    expect(invoke).toHaveBeenCalledWith('open_data_folder');
  });

  it('openBugReportForm', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await diagnosticsApi.openBugReportForm('de', true);
    expect(invoke).toHaveBeenCalledWith('open_bug_report_form', { lang: 'de', withLog: true });
  });

  it('getVerboseLogging', async () => {
    const result = { enabled: false, untilMs: null };
    vi.mocked(invoke).mockResolvedValue(result);
    const actual = await diagnosticsApi.getVerboseLogging();
    expect(invoke).toHaveBeenCalledWith('get_verbose_logging');
    expect(actual).toEqual(result);
  });

  it('setVerboseLogging', async () => {
    const result = { enabled: true, untilMs: 1758888888000 };
    vi.mocked(invoke).mockResolvedValue(result);
    const actual = await diagnosticsApi.setVerboseLogging(true);
    expect(invoke).toHaveBeenCalledWith('set_verbose_logging', { enabled: true });
    expect(actual).toEqual(result);
  });
});

it('reads container paths without invoking folder openers', async () => {
  const paths = { data: '/config/data', logs: '/config/logs' };
  vi.mocked(invoke).mockResolvedValue(paths);
  expect(await diagnosticsApi.getDataPaths()).toEqual(paths);
  expect(invoke).toHaveBeenCalledExactlyOnceWith('get_data_paths');
});
it('requests the report URL with the selected language and log flag', async () => {
  const url = 'https://3mfkatalog.de/en/report-a-bug.html?version=0.16.0&os=linux&log=1';
  vi.mocked(invoke).mockResolvedValue(url);
  expect(await diagnosticsApi.getBugReportUrl('fr', true)).toBe(url);
  expect(invoke).toHaveBeenCalledExactlyOnceWith('get_bug_report_url', { lang: 'fr', withLog: true });
});
