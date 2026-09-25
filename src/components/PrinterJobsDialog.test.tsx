import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { PrinterJobsDialog } from './PrinterJobsDialog';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import type { FilamentSpool, PrinterJob } from '../types';

vi.mock('../lib/api/printerLink', () => ({ getPrinterJobThumbnail: vi.fn().mockResolvedValue(null) }));
beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

const spools = [
  { id: '100', material: 'PLA', color: 'Grau', remainingWeightG: 612.4, originalWeightG: 1000, diameterMm: 1.75, colorHex: '#8a8f94' },
  { id: '101', material: 'PETG', color: 'Petrol', remainingWeightG: 900, originalWeightG: 1000, diameterMm: 1.75, colorHex: '#1f7a7a' },
] as FilamentSpool[];

const job = (over: Partial<PrinterJob>): PrinterJob => ({
  id: '1', printerId: '1', printerName: 'Sovol SV08', fileName: 'Distanzhulse_13,40mm_PLA_0.2_6m29s.gcode', outcome: 'completed',
  rawStatus: 'completed', endedAt: 1789042135, printDurationS: 395, usedMm: 303.3, partialPercent: null, material: 'PLA',
  hasThumbnail: false, suggestedSpoolId: '100', grams: 0.9, materialMismatch: false,
  modelMatch: { fileId: '7', fileName: 'Distanzhülse 13,40mm.3mf', sure: true }, ...over,
});

function link(jobs: PrinterJob[]): PrinterLinkState {
  return {
    enabled: true, connections: [], jobs, error: null, refresh: vi.fn(), setEnabled: vi.fn(), testConnection: vi.fn(),
    removeConnection: vi.fn(), syncNow: vi.fn(), ignoreJob: vi.fn().mockResolvedValue(undefined),
    confirmJobs: vi.fn().mockResolvedValue({ confirmed: 1, failed: 0 }),
    previewJob: vi.fn().mockResolvedValue({ grams: 0.9, materialMismatch: true }),
  } as PrinterLinkState;
}

function renderDialog(l: PrinterLinkState) {
  const onBooked = vi.fn();
  render(
    <LanguageProvider>
      <PrinterJobsDialog open jobs={l.jobs} spools={spools} models={[{ id: '7', name: 'Distanzhülse 13,40mm.3mf' }]} link={l} onClose={vi.fn()} onBooked={onBooked} />
    </LanguageProvider>,
  );
  return onBooked;
}

describe('PrinterJobsDialog', () => {
  it('confirms a job with the suggested spool and model', async () => {
    const l = link([job({})]);
    const onBooked = renderDialog(l);
    expect(screen.getByText('FERTIG')).toBeInTheDocument();
    expect(screen.getByText('Distanzhülse 13,40mm.3mf')).toBeInTheDocument();
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Bestätigen' })));
    expect(l.confirmJobs).toHaveBeenCalledWith([{ jobId: '1', spoolId: '100', fileId: '7' }]);
    expect(onBooked).toHaveBeenCalled();
  });

  it('blocks confirming without a spool', () => {
    renderDialog(link([job({ suggestedSpoolId: null, grams: null })]));
    expect(screen.getByRole('button', { name: 'Bestätigen' })).toBeDisabled();
  });

  it('shows partial jobs with percent', () => {
    renderDialog(link([job({ outcome: 'partial', rawStatus: 'klippy_shutdown', partialPercent: 39, grams: 4 })]));
    expect(screen.getByText('ABGEBROCHEN · 39 %')).toBeInTheDocument();
  });

  it('warns when the material does not match', () => {
    renderDialog(link([job({ material: 'PETG', materialMismatch: true })]));
    expect(screen.getByText(/Drucker meldet PETG, gewählt ist PLA/)).toBeInTheDocument();
  });

  it('confirms all rows that have a spool', async () => {
    const l = link([job({ id: '1' }), job({ id: '2', suggestedSpoolId: null, grams: null })]);
    renderDialog(l);
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Alle 1 bestätigen' })));
    expect(l.confirmJobs).toHaveBeenCalledWith([{ jobId: '1', spoolId: '100', fileId: '7' }]);
  });

  it('ignores a job', async () => {
    const l = link([job({})]);
    renderDialog(l);
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Ignorieren' })));
    expect(l.ignoreJob).toHaveBeenCalledWith('1');
  });

  it('closes with Escape', () => {
    const l = link([job({})]);
    const onClose = vi.fn();
    render(
      <LanguageProvider>
        <PrinterJobsDialog open jobs={l.jobs} spools={spools} models={[]} link={l} onClose={onClose} onBooked={vi.fn()} />
      </LanguageProvider>,
    );
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });
});
