import { invoke } from '@tauri-apps/api/core';

export interface BugReportInfo {
  version: string;
  os: 'linux' | 'windows' | 'macos';
}

export interface LogSegment {
  text: string;
  replaced: boolean;
}

export interface LogPreview {
  segments: LogSegment[];
  containsDebug: boolean;
  replaceFileNames: boolean;
  empty: boolean;
}

export interface VerboseLogging {
  enabled: boolean;
  untilMs: number | null;
}

export const getBugReportInfo = () => invoke<BugReportInfo>('get_bug_report_info');

export const previewLogExport = (replaceFileNames?: boolean) =>
  invoke<LogPreview>('preview_log_export', { replaceFileNames: replaceFileNames ?? null });

export const saveLogExport = () => invoke<string>('save_log_export');

export const openLogFolder = () => invoke<void>('open_log_folder');

export const openBugReportForm = (lang: string, withLog: boolean) =>
  invoke<void>('open_bug_report_form', { lang, withLog });

export const getVerboseLogging = () => invoke<VerboseLogging>('get_verbose_logging');

export const setVerboseLogging = (enabled: boolean) =>
  invoke<VerboseLogging>('set_verbose_logging', { enabled });
