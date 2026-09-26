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

function renderDialog(l: PrinterLinkState, onClose = vi.fn()) {
  const onBooked = vi.fn();
  render(
    <LanguageProvider>
      <PrinterJobsDialog open jobs={l.jobs} spools={spools} models={[{ id: '7', name: 'Distanzhülse 13,40mm.3mf' }]} link={l} onClose={onClose} onBooked={onBooked} />
    </LanguageProvider>,
  );
  return onBooked;
}

/** Controllable promise for "a request is still running" in the guard tests. */
function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
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

  it('ignores a resin bottle as booking candidate, even as the suggestion', () => {
    const withResin = [
      ...spools,
      { id: '200', kind: 'resin', material: 'Standard', color: 'Grau', remainingWeightG: 640.5, originalWeightG: 1000, diameterMm: 1.75, colorHex: '#8a8f98' },
    ] as FilamentSpool[];
    const l = link([job({ suggestedSpoolId: '200' })]);
    render(
      <LanguageProvider>
        <PrinterJobsDialog open jobs={l.jobs} spools={withResin} models={[]} link={l} onClose={vi.fn()} onBooked={vi.fn()} />
      </LanguageProvider>,
    );
    // No suggestion taken -> can't be confirmed without a spool.
    expect(screen.getByRole('button', { name: 'Bestätigen' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    expect(screen.queryByRole('option', { name: /Standard/ })).toBeNull();
    expect(screen.getAllByRole('option')).toHaveLength(2);
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

  it('does not close the dialog when Escape is pressed inside the open spool picker', () => {
    const l = link([job({})]);
    const onClose = vi.fn();
    renderDialog(l, onClose);
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    fireEvent.keyDown(screen.getByRole('listbox'), { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });

  it('disables Bestätigen while a confirm is in flight and does not call confirmJobs twice', async () => {
    const l = link([job({})]);
    const d = deferred<{ confirmed: number; failed: number }>();
    l.confirmJobs = vi.fn().mockReturnValue(d.promise);
    renderDialog(l);
    const button = screen.getByRole('button', { name: 'Bestätigen' });
    fireEvent.click(button);
    expect(button).toBeDisabled();
    fireEvent.click(button);
    expect(l.confirmJobs).toHaveBeenCalledTimes(1);
    await act(async () => d.resolve({ confirmed: 1, failed: 0 }));
  });

  it('disables "Alle N bestätigen" while one of its rows is already being confirmed', async () => {
    const l = link([job({ id: '1' }), job({ id: '2' })]);
    const d = deferred<{ confirmed: number; failed: number }>();
    l.confirmJobs = vi.fn().mockReturnValue(d.promise);
    renderDialog(l);
    fireEvent.click(screen.getAllByRole('button', { name: 'Bestätigen' })[0]);
    expect(screen.getByRole('button', { name: 'Alle 2 bestätigen' })).toBeDisabled();
    await act(async () => d.resolve({ confirmed: 1, failed: 0 }));
    expect(screen.getByRole('button', { name: 'Alle 2 bestätigen' })).not.toBeDisabled();
  });

  it('disables Ignorieren while an ignore is in flight and does not call ignoreJob twice', async () => {
    const l = link([job({})]);
    const d = deferred<void>();
    l.ignoreJob = vi.fn().mockReturnValue(d.promise);
    renderDialog(l);
    const button = screen.getByRole('button', { name: 'Ignorieren' });
    fireEvent.click(button);
    expect(button).toBeDisabled();
    fireEvent.click(button);
    expect(l.ignoreJob).toHaveBeenCalledTimes(1);
    await act(async () => d.resolve());
  });

  it('shows an error message when confirming is rejected', async () => {
    const l = link([job({})]);
    l.confirmJobs = vi.fn().mockRejectedValue(new Error('Netzwerk kaputt'));
    renderDialog(l);
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Bestätigen' })));
    expect(screen.getByRole('alert')).toHaveTextContent('Das hat nicht geklappt: Netzwerk kaputt');
  });

  it('shows how many prints could not be booked when confirmJobs reports failures', async () => {
    const l = link([job({})]);
    l.confirmJobs = vi.fn().mockResolvedValue({ confirmed: 0, failed: 1 });
    renderDialog(l);
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Bestätigen' })));
    expect(screen.getByText('1 Druck konnte nicht gebucht werden.')).toBeInTheDocument();
  });

  it('clears a previous "could not be booked" message once a new confirm succeeds', async () => {
    const l = link([job({})]);
    l.confirmJobs = vi
      .fn()
      .mockResolvedValueOnce({ confirmed: 0, failed: 1 })
      .mockResolvedValueOnce({ confirmed: 1, failed: 0 });
    renderDialog(l);
    const button = screen.getByRole('button', { name: 'Bestätigen' });
    await act(async () => fireEvent.click(button));
    expect(screen.getByText('1 Druck konnte nicht gebucht werden.')).toBeInTheDocument();
    await act(async () => fireEvent.click(button));
    expect(screen.queryByText('1 Druck konnte nicht gebucht werden.')).not.toBeInTheDocument();
  });

  it('ignores a stale preview response for a spool that is no longer selected', async () => {
    const l = link([job({ suggestedSpoolId: null, grams: null, materialMismatch: false })]);
    const first = deferred<{ grams: number; materialMismatch: boolean }>();
    const second = deferred<{ grams: number; materialMismatch: boolean }>();
    let call = 0;
    l.previewJob = vi.fn().mockImplementation(() => {
      call += 1;
      return call === 1 ? first.promise : second.promise;
    });
    renderDialog(l);

    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    fireEvent.mouseDown(screen.getByRole('option', { name: 'PETG · Petrol · 900,0 g' })); // slow first request for spool 101
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    fireEvent.mouseDown(screen.getByRole('option', { name: 'PLA · Grau · 612,4 g' })); // faster second request for spool 100

    await act(async () => second.resolve({ grams: 0.9, materialMismatch: false }));
    expect(screen.getByText('0,9 g')).toBeInTheDocument();

    // The stale answer for spool 101, left in the meantime,
    // must no longer overwrite the current state (spool 100).
    await act(async () => first.resolve({ grams: 5, materialMismatch: true }));
    expect(screen.getByText('0,9 g')).toBeInTheDocument();
    expect(screen.queryByText(/Drucker meldet/)).not.toBeInTheDocument();
  });

  it('clears a row\'s spool selection once that spool no longer exists in the catalog', async () => {
    // e.g. after a backup restore with fewer spools - without
    // this guard one could "confirm" with a spool that no longer
    // exists.
    const l = link([job({})]);
    const { rerender } = render(
      <LanguageProvider>
        <PrinterJobsDialog open jobs={l.jobs} spools={spools} models={[]} link={l} onClose={vi.fn()} onBooked={vi.fn()} />
      </LanguageProvider>,
    );
    expect(screen.getByRole('button', { name: 'Bestätigen' })).not.toBeDisabled();

    const spoolsWithout100 = spools.filter((s) => s.id !== '100');
    rerender(
      <LanguageProvider>
        <PrinterJobsDialog open jobs={l.jobs} spools={spoolsWithout100} models={[]} link={l} onClose={vi.fn()} onBooked={vi.fn()} />
      </LanguageProvider>,
    );
    expect(screen.getByRole('button', { name: 'Bestätigen' })).toBeDisabled();
    expect(screen.getByRole('button', { name: /Spule wählen/ })).toBeInTheDocument();
  });

  it('applies the suggested spool once it appears in a later spools update (spools loads after jobs)', () => {
    const l = link([job({ suggestedSpoolId: '100' })]);
    const { rerender } = render(
      <LanguageProvider>
        <PrinterJobsDialog open jobs={l.jobs} spools={[]} models={[]} link={l} onClose={vi.fn()} onBooked={vi.fn()} />
      </LanguageProvider>,
    );
    // On the first render `spools` is still empty - the suggestion can't
    // be applied yet.
    expect(screen.getByRole('button', { name: 'Bestätigen' })).toBeDisabled();
    expect(screen.getByRole('button', { name: /Spule wählen/ })).toBeInTheDocument();

    rerender(
      <LanguageProvider>
        <PrinterJobsDialog open jobs={l.jobs} spools={spools} models={[]} link={l} onClose={vi.fn()} onBooked={vi.fn()} />
      </LanguageProvider>,
    );
    // Now spool 100 is present in `spools` - the suggestion must
    // be applied afterwards.
    expect(screen.getByRole('button', { name: 'Bestätigen' })).not.toBeDisabled();
    expect(screen.getByRole('button', { name: /PLA · Grau/ })).toBeInTheDocument();
  });

  it("keeps the user's own spool choice when the spools list is refreshed (not overwritten by the suggestion)", () => {
    const l = link([job({ suggestedSpoolId: '100' })]);
    const { rerender } = render(
      <LanguageProvider>
        <PrinterJobsDialog open jobs={l.jobs} spools={spools} models={[]} link={l} onClose={vi.fn()} onBooked={vi.fn()} />
      </LanguageProvider>,
    );
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    fireEvent.mouseDown(screen.getByRole('option', { name: 'PETG · Petrol · 900,0 g' })); // deliberately NOT the suggestion (spool 100)
    expect(screen.getByRole('button', { name: /PETG · Petrol/ })).toBeInTheDocument();

    // A new `spools` array (e.g. after a background sync) must not
    // replace the user's choice with the suggestion.
    const refreshedSpools = spools.map((s) => ({ ...s }));
    rerender(
      <LanguageProvider>
        <PrinterJobsDialog open jobs={l.jobs} spools={refreshedSpools} models={[]} link={l} onClose={vi.fn()} onBooked={vi.fn()} />
      </LanguageProvider>,
    );
    expect(screen.getByRole('button', { name: /PETG · Petrol/ })).toBeInTheDocument();
  });
});
