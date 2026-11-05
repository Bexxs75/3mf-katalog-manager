import { invoke } from '../transport';
import { parseSlicerReports, parseSlicerReportSetup } from '../slicerReports';

export const SLICER_REPORTS_EVENT = 'slicer-reports-changed';

export async function getSlicerReportSetup() {
  return parseSlicerReportSetup(await invoke<unknown>('get_slicer_report_setup'));
}
/** Switching on remembers the program path of the command lines; switching on again after a move renews it. */
export async function setSlicerReportEnabled(enabled: boolean) {
  return parseSlicerReportSetup(await invoke<unknown>('set_slicer_report_enabled', { enabled }));
}
export function sendSlicerTestReport() {
  return invoke<void>('send_slicer_test_report');
}
export async function listSlicerReports() {
  return parseSlicerReports(await invoke<unknown>('list_slicer_reports'));
}
/** `link` needs the model; `dismiss` does not. The file itself is not touched. */
export function resolveSlicerReport(id: string, action: 'link' | 'dismiss', modelFileId?: string) {
  return invoke<void>('resolve_slicer_report', { id, action, modelFileId: modelFileId ?? null });
}
