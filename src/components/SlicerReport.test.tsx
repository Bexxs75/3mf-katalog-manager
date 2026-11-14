import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { SlicerReportsProvider, useSlicerReports } from '../hooks/SlicerReportsContext';
import { SlicerReportSettings } from './SlicerReportSettings';
import { SlicerReportInbox } from './SlicerReportInbox';
import { SlicerReportNotice } from './SlicerReportNotice';
import { de } from '../i18n/de';
import { LanguageProviderWithDiagnostics } from '../test/renderWithDiagnostics';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));

const LINE = '"/opt/app" --report-sliced';
const SLICERS = ['PrusaSlicer', 'SuperSlicer', 'OrcaSlicer', 'Bambu Studio', 'Creality Print'];
const model = (id: string, name: string, fileType = '3mf') => ({ id, name, fileType });
const MODELS = [model('7', 'Box.3mf'), model('8', 'Vase.3mf'), model('9', 'Box.gcode', 'gcode')];

let setup: { enabled: boolean; programPath: string; pathChanged: boolean; openCount: number; lastError: string | null; commands: unknown[] };
let reports: Array<Record<string, unknown>>;
let failing: Record<string, unknown> = {};

const report = (over: Record<string, unknown> = {}) => ({
  id: '1', reportedAt: '2026-11-05T10:00:00Z', reportedPath: '/home/u/out/Box_PETG_0.2_1h10m.gcode', fileName: 'Box_PETG_0.2_1h10m.gcode', fileSizeBytes: 2048,
  slicer: 'PrusaSlicer 2.8.1', printSeconds: 4200, weightG: 12.34, filamentTypes: ['PETG'], state: 'open', isTest: false,
  suggestion: { fileId: '7', fileName: 'Box.3mf', sure: true }, model: null, decidedAt: null, ...over,
});

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  failing = {};
  setup = { enabled: true, programPath: '/opt/app', pathChanged: false, openCount: 0, lastError: null, commands: SLICERS.map(slicer => ({ slicer, commandLine: LINE })) };
  reports = [report()];
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    const p = (args ?? {}) as Record<string, unknown>;
    if (failing[cmd]) throw failing[cmd];
    switch (cmd) {
      case 'get_slicer_report_setup': return structuredClone(setup);
      case 'set_slicer_report_enabled': setup = { ...setup, enabled: p.enabled as boolean, pathChanged: false }; return structuredClone(setup);
      case 'send_slicer_test_report': reports = [report({ id: '99', fileName: 'Test report.gcode', reportedPath: '/x/Test report.gcode', isTest: true, suggestion: null }), ...reports]; return undefined;
      case 'list_slicer_reports': return structuredClone(reports);
      case 'resolve_slicer_report': {
        reports = reports.map(r => r.id === p.id ? { ...r, state: p.action === 'link' ? 'linked' : 'dismissed', model: p.action === 'link' ? { fileId: p.modelFileId, fileName: 'x', sure: true } : null } : r);
        return undefined;
      }
      default: return undefined;
    }
  });
});

const wrap = (ui: ReactNode) => render(<LanguageProvider><SlicerReportsProvider>{ui}</SlicerReportsProvider></LanguageProvider>);
const calls = (name: string) => vi.mocked(invoke).mock.calls.filter(c => c[0] === name);

// Failures are unexpected errors here, which add a report link that needs the diagnostics context.
const wrapDiag = (ui: ReactNode) => render(<LanguageProviderWithDiagnostics><SlicerReportsProvider>{ui}</SlicerReportsProvider></LanguageProviderWithDiagnostics>);

describe('settings load and save states', () => {
  it('reports a failed read instead of leaving a dead switch, and reads again on request', async () => {
    failing = { get_slicer_report_setup: new Error('boom') };
    wrapDiag(<SlicerReportSettings />);
    expect(await screen.findByRole('alert')).toHaveTextContent(de.settingLoadFailed);
    expect(screen.getByRole('switch', { name: de.srTitle })).toBeDisabled();
    failing = {};
    fireEvent.click(screen.getByRole('button', { name: de.settingReload }));
    await waitFor(() => expect(screen.getByRole('switch', { name: de.srTitle })).toBeEnabled());
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('keeps the confirmed setup and reports a failed save', async () => {
    failing = { set_slicer_report_enabled: new Error('boom') };
    wrapDiag(<SlicerReportSettings />);
    const toggle = await screen.findByRole('switch', { name: de.srTitle });
    await waitFor(() => expect(toggle).toBeEnabled());
    fireEvent.click(toggle);
    expect(await screen.findByRole('alert')).toHaveTextContent(de.settingSaveFailedKept);
    expect(toggle).toBeChecked();
  });
});

describe('settings', () => {
  it('shows the switch, one command line per slicer and the two steps', async () => {
    wrap(<SlicerReportSettings />);
    const toggle = await screen.findByRole('switch', { name: de.srTitle });
    expect(toggle).toBeChecked();
    for (const slicer of SLICERS) expect(await screen.findByLabelText(`Befehlszeile für ${slicer}`)).toHaveValue(LINE);
    expect(screen.getAllByRole('listitem').length).toBeGreaterThanOrEqual(2 + SLICERS.length);
  });

  it('switching on and off goes through the backend and hides the lines while off', async () => {
    setup.enabled = false;
    wrap(<SlicerReportSettings />);
    const toggle = await screen.findByRole('switch');
    expect(toggle).not.toBeChecked();
    expect(screen.getByText(/Die Meldung ist aus/)).toBeInTheDocument();
    expect(screen.queryByLabelText('Befehlszeile für PrusaSlicer')).toBeNull();
    fireEvent.click(toggle);
    await waitFor(() => expect(calls('set_slicer_report_enabled')[0][1]).toEqual({ enabled: true }));
    expect(await screen.findByLabelText('Befehlszeile für PrusaSlicer')).toBeInTheDocument();
  });

  it('copies a command line and says so; a refused clipboard selects the line instead', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    wrap(<SlicerReportSettings />);
    const row = (await screen.findByLabelText('Befehlszeile für OrcaSlicer')).parentElement!;
    fireEvent.click(within(row).getByRole('button', { name: 'Kopieren' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(LINE));
    expect(await screen.findByText('Befehlszeile für OrcaSlicer kopiert.')).toBeInTheDocument();
    writeText.mockRejectedValueOnce(new Error('no'));
    fireEvent.click(within(row).getByRole('button', { name: 'Kopieren' }));
    expect(await screen.findByText(/Kopieren war nicht möglich/)).toBeInTheDocument();
  });

  it('warns when the program path changed and renews the lines on request', async () => {
    setup.pathChanged = true;
    wrap(<SlicerReportSettings />);
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Der Programmpfad hat sich geändert.');
    fireEvent.click(within(alert).getByRole('button', { name: 'Befehlszeilen neu erzeugen' }));
    await waitFor(() => expect(calls('set_slicer_report_enabled')[0][1]).toEqual({ enabled: true }));
    expect(await screen.findByText('Befehlszeilen neu erzeugt. Bitte in den Slicern ersetzen.')).toBeInTheDocument();
    expect(screen.queryByText('Der Programmpfad hat sich geändert.')).toBeNull();
  });

  it('sends a test report and tells where it shows up; a failure is reported', async () => {
    wrap(<SlicerReportSettings />);
    fireEvent.click(await screen.findByRole('button', { name: 'Testmeldung senden' }));
    expect(await screen.findByText(/Testmeldung gesendet/)).toBeInTheDocument();
    failing = { send_slicer_test_report: { message: 'slicerReportActionInvalid', expected: true } };
    fireEvent.click(screen.getByRole('button', { name: 'Testmeldung senden' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Die Testmeldung konnte nicht gesendet werden.');
  });

  it('names a path that cannot be a command line and shows the last failed call', async () => {
    setup.commands = [{ slicer: 'PrusaSlicer', commandLine: null }];
    setup.lastError = '2026-11-05 the file does not exist';
    wrap(<SlicerReportSettings />);
    expect(await screen.findByText(/lässt sich keine Befehlszeile erzeugen/)).toBeInTheDocument();
    expect(screen.getByText(/Letzter fehlgeschlagener Aufruf/)).toHaveTextContent('the file does not exist');
  });
});

function Harness({ onImport = vi.fn().mockResolvedValue(undefined) }: { onImport?: (paths: string[]) => Promise<void> }) {
  const { openInbox, inboxOpen } = useSlicerReports();
  return <>
    <button onClick={openInbox}>open</button>
    <SlicerReportNotice />
    {inboxOpen && <SlicerReportInbox models={MODELS} onImportPaths={onImport} />}
  </>;
}

describe('notice and inbox', () => {
  it('shows a notice for new reports; closing it keeps it away until a newer one arrives', async () => {
    wrap(<Harness />);
    const notice = await screen.findByRole('region', { name: 'Neue Slicer-Dateien' });
    expect(notice).toHaveTextContent('Ein Slicer hat eine neue Datei gemeldet.');
    fireEvent.click(within(notice).getByRole('button', { name: 'Schließen' }));
    expect(screen.queryByRole('region', { name: 'Neue Slicer-Dateien' })).toBeNull();
  });

  it('lists a report with its facts, the suggestion marked sure, and the explicit tick for the catalog', async () => {
    wrap(<Harness />);
    fireEvent.click(await screen.findByRole('button', { name: 'Ansehen' }));
    const dialog = await screen.findByRole('dialog', { name: 'Neue Slicer-Dateien' });
    expect(dialog).toHaveTextContent('Box_PETG_0.2_1h10m.gcode');
    expect(dialog).toHaveTextContent('1 h 10 min');
    expect(dialog).toHaveTextContent('12,3 g');
    expect(dialog).toHaveTextContent('Vorschlag: Box.3mf (sicher)');
    expect(dialog).toHaveTextContent('/home/u/out/Box_PETG_0.2_1h10m.gcode');
    const tick = within(dialog).getByRole('checkbox', { name: /Auch in den Katalog aufnehmen/ });
    expect(tick).not.toBeChecked();
    expect(screen.queryByRole('note')).toBeNull();
    fireEvent.click(tick);
    expect(screen.getByRole('note')).toHaveTextContent('aus dem Ausgabeordner des Slicers in den Katalog verschoben');
  });

  it('marks an unsure suggestion and asks for a choice when there is none', async () => {
    reports = [report({ suggestion: { fileId: '8', fileName: 'Vase.3mf', sure: false } }), report({ id: '2', fileName: 'x.gcode', reportedPath: '/o/x.gcode', suggestion: null })];
    wrap(<Harness />);
    fireEvent.click(await screen.findByRole('button', { name: 'open' }));
    const dialog = await screen.findByRole('dialog');
    expect(dialog).toHaveTextContent('Vorschlag: Vase.3mf (unsicher)');
    expect(dialog).toHaveTextContent('Kein passendes Modell gefunden. Wähle eines aus.');
    const items = within(dialog).getAllByRole('listitem').filter(li => li.hasAttribute('data-sr-report'));
    expect(within(items[1]).queryByRole('button', { name: 'Zuordnen' })).toBeNull();
    expect(within(items[1]).queryByRole('checkbox')).toBeNull();
  });

  it('assigns the suggested model without touching the file and says where the file stays', async () => {
    const onImport = vi.fn().mockResolvedValue(undefined);
    wrap(<Harness onImport={onImport} />);
    fireEvent.click(await screen.findByRole('button', { name: 'open' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Zuordnen' }));
    await waitFor(() => expect(calls('resolve_slicer_report')[0][1]).toEqual({ id: '1', action: 'link', modelFileId: '7' }));
    const status = await screen.findByText(/wurde „Box.3mf“ zugeordnet/);
    expect(status).toHaveTextContent('Die Datei bleibt im Slicer-Ordner.');
    expect(onImport).not.toHaveBeenCalled();
    expect(screen.getByText('Keine neuen Slicer-Dateien')).toBeInTheDocument();
    await waitFor(() => expect(status).toHaveFocus());
  });

  it('adds the file to the catalog only with the tick, and keeps the link when the import fails', async () => {
    const onImport = vi.fn().mockRejectedValue({ message: 'boom', expected: true });
    wrap(<Harness onImport={onImport} />);
    fireEvent.click(await screen.findByRole('button', { name: 'open' }));
    fireEvent.click(await screen.findByRole('checkbox', { name: /Auch in den Katalog aufnehmen/ }));
    fireEvent.click(screen.getByRole('button', { name: 'Zuordnen' }));
    await waitFor(() => expect(onImport).toHaveBeenCalledWith(['/home/u/out/Box_PETG_0.2_1h10m.gcode']));
    expect(await screen.findByRole('alert')).toHaveTextContent('Die Datei konnte nicht in den Katalog aufgenommen werden. Die Zuordnung ist gespeichert.');
    expect(calls('resolve_slicer_report')).toHaveLength(1);
  });

  it('lets the user choose another model and never offers G-code entries', async () => {
    wrap(<Harness />);
    fireEvent.click(await screen.findByRole('button', { name: 'open' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Anderes Modell wählen' }));
    const options = await screen.findAllByRole('option');
    expect(options.map(o => o.textContent).join('|')).toContain('Vase.3mf');
    expect(options.map(o => o.textContent).join('|')).not.toContain('Box.gcode');
    fireEvent.click(options.find(o => o.textContent?.includes('Vase.3mf'))!);
    expect(await screen.findByText('Gewähltes Modell: Vase.3mf')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Zuordnen' }));
    await waitFor(() => expect(calls('resolve_slicer_report')[0][1]).toEqual({ id: '1', action: 'link', modelFileId: '8' }));
  });

  it('dismisses a report and offers only dismissal for a test report', async () => {
    reports = [report({ id: '2', isTest: true, suggestion: null, fileName: 'Test report.gcode', reportedPath: '/x/Test report.gcode' }), report()];
    wrap(<Harness />);
    fireEvent.click(await screen.findByRole('button', { name: 'open' }));
    const dialog = await screen.findByRole('dialog');
    const items = within(dialog).getAllByRole('listitem').filter(li => li.hasAttribute('data-sr-report'));
    expect(items[0]).toHaveTextContent('Testmeldung');
    expect(within(items[0]).queryByRole('button', { name: 'Zuordnen' })).toBeNull();
    fireEvent.click(within(items[0]).getByRole('button', { name: 'Verwerfen' }));
    await waitFor(() => expect(calls('resolve_slicer_report')[0][1]).toEqual({ id: '2', action: 'dismiss', modelFileId: null }));
    expect(await screen.findByText('Test report.gcode wurde verworfen.')).toBeInTheDocument();
  });

  it('reports a failed decision with the reason and leaves the report in place', async () => {
    failing = { resolve_slicer_report: { message: 'slicerReportNotFound', expected: true } };
    wrap(<Harness />);
    fireEvent.click(await screen.findByRole('button', { name: 'open' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Verwerfen' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Das hat nicht geklappt.');
    expect(screen.getByRole('alert')).toHaveTextContent('Diese Meldung gibt es nicht mehr');
    expect(screen.getByText('Box_PETG_0.2_1h10m.gcode')).toBeInTheDocument();
  });

  it('shows the empty state and the recently handled reports', async () => {
    reports = [report({ state: 'linked', model: { fileId: '7', fileName: 'Box.3mf', sure: true } })];
    wrap(<Harness />);
    fireEvent.click(await screen.findByRole('button', { name: 'open' }));
    expect(await screen.findByText('Keine neuen Slicer-Dateien', { selector: 'div' })).toBeInTheDocument();
    expect(screen.getByText(/Zuletzt bearbeitet \(1\)/)).toBeInTheDocument();
    expect(screen.getByText(/zugeordnet: Box.3mf/)).toBeInTheDocument();
  });
});

it('keeps local settings mounted without reading setup while its tab is inactive', async () => {
  const ui = (active: boolean) => <LanguageProvider><SlicerReportSettings active={active} /></LanguageProvider>;
  const view = render(ui(true));
  await waitFor(() => expect(calls('get_slicer_report_setup')).toHaveLength(1));
  await screen.findByRole('switch');
  view.rerender(ui(false));
  expect(calls('get_slicer_report_setup')).toHaveLength(1);
  view.rerender(ui(false));
  expect(calls('get_slicer_report_setup')).toHaveLength(1);
  view.rerender(ui(true));
  await waitFor(() => expect(calls('get_slicer_report_setup')).toHaveLength(2));
});
