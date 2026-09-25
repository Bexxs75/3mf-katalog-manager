import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

import * as api from './printerLink';

beforeEach(() => invoke.mockReset().mockResolvedValue(undefined));

describe('printerLink api', () => {
  it('tests a moonraker connection with camelCase arguments', async () => {
    await api.testPrinterConnection('1', '192.168.1.60');
    expect(invoke).toHaveBeenCalledWith('test_printer_connection', { printerId: '1', kind: 'moonraker', address: '192.168.1.60' });
  });
  it('sends all decisions at once', async () => {
    await api.confirmPrinterJobs([{ jobId: '5', spoolId: '100', fileId: null }]);
    expect(invoke).toHaveBeenCalledWith('confirm_printer_jobs', { decisions: [{ jobId: '5', spoolId: '100', fileId: null }] });
  });
  it('reads the switch state', async () => {
    invoke.mockResolvedValue(true);
    const actual = await api.getPrinterLinkEnabled();
    expect(invoke).toHaveBeenCalledWith('get_printer_link_enabled');
    expect(actual).toBe(true);
  });
  it('sets the switch state', async () => {
    await api.setPrinterLinkEnabled(false);
    expect(invoke).toHaveBeenCalledWith('set_printer_link_enabled', { enabled: false });
  });
  it('lists connections', async () => {
    await api.listPrinterConnections();
    expect(invoke).toHaveBeenCalledWith('list_printer_connections');
  });
  it('removes a connection', async () => {
    await api.removePrinterConnection('1');
    expect(invoke).toHaveBeenCalledWith('remove_printer_connection', { printerId: '1' });
  });
  it('triggers a manual sync', async () => {
    await api.syncPrintersNow();
    expect(invoke).toHaveBeenCalledWith('sync_printers_now');
  });
  it('lists open jobs', async () => {
    await api.listOpenPrinterJobs();
    expect(invoke).toHaveBeenCalledWith('list_open_printer_jobs');
  });
  it('previews a job booking', async () => {
    await api.previewPrinterJob('5', '100');
    expect(invoke).toHaveBeenCalledWith('preview_printer_job', { jobId: '5', spoolId: '100' });
  });
  it('fetches a job thumbnail', async () => {
    await api.getPrinterJobThumbnail('5');
    expect(invoke).toHaveBeenCalledWith('get_printer_job_thumbnail', { jobId: '5' });
  });
  it('ignores a job', async () => {
    await api.ignorePrinterJob('5');
    expect(invoke).toHaveBeenCalledWith('ignore_printer_job', { jobId: '5' });
  });
  it('exposes the sync event name', () => {
    expect(api.PRINTER_JOBS_CHANGED_EVENT).toBe('printer-jobs-changed');
  });
});
