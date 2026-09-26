import { invoke } from '@tauri-apps/api/core';
import type {
  ConfirmResult,
  JobDecision,
  JobPreview,
  PrinterConnection,
  PrinterJob,
  PrinterTestResult,
} from '../../types';

/** Sent by the backend after every sync. */
export const PRINTER_JOBS_CHANGED_EVENT = 'printer-jobs-changed';

export function getPrinterLinkEnabled() {
  return invoke<boolean>('get_printer_link_enabled');
}
export function setPrinterLinkEnabled(enabled: boolean) {
  return invoke<void>('set_printer_link_enabled', { enabled });
}
export function listPrinterConnections() {
  return invoke<PrinterConnection[]>('list_printer_connections');
}
/** Klipper/Moonraker only for now. */
export function testPrinterConnection(printerId: string, address: string) {
  return invoke<PrinterTestResult>('test_printer_connection', { printerId, kind: 'moonraker', address });
}
export function removePrinterConnection(printerId: string) {
  return invoke<void>('remove_printer_connection', { printerId });
}
export function syncPrintersNow() {
  return invoke<void>('sync_printers_now');
}
export function listOpenPrinterJobs() {
  return invoke<PrinterJob[]>('list_open_printer_jobs');
}
export function previewPrinterJob(jobId: string, spoolId: string) {
  return invoke<JobPreview>('preview_printer_job', { jobId, spoolId });
}
/** Base64 PNG or null. */
export function getPrinterJobThumbnail(jobId: string) {
  return invoke<string | null>('get_printer_job_thumbnail', { jobId });
}
export function ignorePrinterJob(jobId: string) {
  return invoke<void>('ignore_printer_job', { jobId });
}
export function confirmPrinterJobs(decisions: JobDecision[]) {
  return invoke<ConfirmResult>('confirm_printer_jobs', { decisions });
}
