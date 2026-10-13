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
/** Each fetch opens its own connection in the backend; hundreds at once exhaust file handles. */
const MAX_CONCURRENT_THUMBNAILS = 4;
let runningThumbnails = 0;
const waitingThumbnails: Array<() => void> = [];

async function acquireThumbnailSlot() {
  if (runningThumbnails >= MAX_CONCURRENT_THUMBNAILS) {
    // The slot is handed over directly, so the counter stays unchanged.
    await new Promise<void>((resolve) => waitingThumbnails.push(resolve));
  } else {
    runningThumbnails++;
  }
}

function releaseThumbnailSlot() {
  const next = waitingThumbnails.shift();
  if (next) next();
  else runningThumbnails--;
}

/** Base64 PNG or null. Runs at most 4 at a time, in request order. */
export async function getPrinterJobThumbnail(jobId: string) {
  await acquireThumbnailSlot();
  try {
    return await invoke<string | null>('get_printer_job_thumbnail', { jobId });
  } finally {
    releaseThumbnailSlot();
  }
}
export function ignorePrinterJob(jobId: string) {
  return invoke<void>('ignore_printer_job', { jobId });
}
export function confirmPrinterJobs(decisions: JobDecision[]) {
  return invoke<ConfirmResult>('confirm_printer_jobs', { decisions });
}
