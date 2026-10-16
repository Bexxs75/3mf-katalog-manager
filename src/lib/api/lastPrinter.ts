import { invoke } from '../transport';
export interface LastPrinter { printerName: string; endedAt: number }
export function getLastPrinterForFile(fileId: string) {
  return invoke<LastPrinter | null>('get_last_printer_for_file', { fileId });
}
