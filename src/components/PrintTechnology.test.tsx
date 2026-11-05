import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { renderHook } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { PrintTechnologyProvider } from '../hooks/PrintTechnologyContext';
import { useFilamentCheck } from '../hooks/useFilamentCheck';
import { usePrinters } from '../hooks/usePrinters';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import type { Printer } from '../types';
import { FilamentView } from './FilamentView';
import { PrinterManagerView } from './PrinterManagerView';
import { PrintTechnologyPrompt } from './PrintTechnologyPrompt';
import { PrintTechnologySettings } from './PrintTechnologySettings';
import { Rail } from './Rail';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: () => Promise.resolve(() => {}) }) }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));

const details = { manufacturer: null, model: null, nozzleMm: null, bedXMm: null, bedYMm: null, bedZMm: null };
const QIDI: Printer = { ...details, id: '1', name: 'Qidi', kind: 'filament', units: [{ id: 'u1', printerId: '1', name: 'Holder', kind: 'external', slotCount: 1, bambuAmsIndex: null }] };
const SATURN: Printer = { ...details, id: '2', name: 'Saturn', kind: 'resin', units: [{ id: 'vat', printerId: '2', name: 'Harzwanne', kind: 'resin_vat', slotCount: 1, bambuAmsIndex: null }] };
const link = () => ({ refreshKey: 0, enabled: false, connections: [], jobs: [], error: null, refresh: vi.fn(), setEnabled: vi.fn(), testConnection: vi.fn(),
  removeConnection: vi.fn(), syncNow: vi.fn(), ignoreJob: vi.fn(), confirmJobs: vi.fn(), previewJob: vi.fn() }) as unknown as PrinterLinkState;

let stored: { value: string; asked: boolean };
let failSave = false;

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  localStorage.removeItem('3mf-katalog-filament-kind');
  stored = { value: 'both', asked: false };
  failSave = false;
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    const p = args as Record<string, unknown>;
    switch (cmd) {
      case 'get_print_technology': return { ...stored };
      case 'set_print_technology':
        if (failSave) throw { message: 'printTechnologyInvalid', expected: true };
        stored = { value: p.value as string, asked: true };
        return { ...stored };
      case 'list_printers': return [structuredClone(QIDI), structuredClone(SATURN)];
      case 'list_filament_spools': return [];
      case 'list_printer_history': return [];
      case 'list_file_summaries': return [];
      case 'check_filament': return [];
      default: return undefined;
    }
  });
});

const wrap = (ui: ReactNode) => render(<LanguageProvider><PrintTechnologyProvider>{ui}</PrintTechnologyProvider></LanguageProvider>);
const choose = async (name: string) => { fireEvent.click(await screen.findByRole('radio', { name: new RegExp(`^${name}`) })); };

describe('settings group', () => {
  it('saves a choice at once, says so, and keeps the group reachable as radios', async () => {
    stored.asked = true;
    wrap(<PrintTechnologySettings />);
    expect(await screen.findByRole('radiogroup', { name: 'Womit druckst du?' })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /^Beides/ })).toBeChecked();
    await choose('Resin');
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('set_print_technology', { value: 'resin' }));
    expect(await screen.findByRole('status')).toHaveTextContent('Gespeichert: Resin.');
    expect(screen.getByRole('radio', { name: /^Resin/ })).toBeChecked();
    expect(screen.getByRole('group')).toHaveTextContent('Resin: Filament-Bereiche sind ausgeblendet. Nichts wird gelöscht');
  });

  it('keeps the shown choice and reports when saving fails', async () => {
    stored.asked = true;
    failSave = true;
    wrap(<PrintTechnologySettings />);
    await choose('Filament');
    expect(await screen.findByRole('alert')).toHaveTextContent('Die Auswahl konnte nicht gespeichert werden.');
    expect(screen.getByRole('radio', { name: /^Beides/ })).toBeChecked();
  });
});

describe('start-up question', () => {
  it('asks a new installation with a dialog and stores the applied choice', async () => {
    wrap(<PrintTechnologyPrompt firstRun wait={false} onChoose={vi.fn()} />);
    const dialog = await screen.findByRole('dialog', { name: 'Womit druckst du?' });
    expect(dialog).toBeInTheDocument();
    await choose('Filament');
    expect(invoke).not.toHaveBeenCalledWith('set_print_technology', expect.anything());
    fireEvent.click(screen.getByRole('button', { name: 'Übernehmen' }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('set_print_technology', { value: 'filament' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });

  it('counts closing the dialog as answered and keeps both', async () => {
    wrap(<PrintTechnologyPrompt firstRun wait={false} onChoose={vi.fn()} />);
    await screen.findByRole('dialog');
    fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('set_print_technology', { value: 'both' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });

  it('only shows a notice to an existing installation, never a dialog', async () => {
    const onChoose = vi.fn();
    wrap(<PrintTechnologyPrompt firstRun={false} wait={false} onChoose={onChoose} />);
    const region = await screen.findByRole('region', { name: 'Womit druckst du?' });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith('set_print_technology', expect.anything());
    fireEvent.click(screen.getByRole('button', { name: 'Jetzt auswählen' }));
    expect(onChoose).toHaveBeenCalled();
    expect(region).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Schließen' }));
    await waitFor(() => expect(screen.queryByRole('region')).toBeNull());
    expect(stored).toEqual({ value: 'both', asked: true });
  });

  it('stays quiet while another question is open and once the question was answered', async () => {
    const { rerender } = wrap(<PrintTechnologyPrompt firstRun wait onChoose={vi.fn()} />);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_print_technology'));
    expect(screen.queryByRole('dialog')).toBeNull();
    rerender(<LanguageProvider><PrintTechnologyProvider><PrintTechnologyPrompt firstRun wait={false} onChoose={vi.fn()} /></PrintTechnologyProvider></LanguageProvider>);
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
  });

  it('does not ask when the answer was given before', async () => {
    stored = { value: 'resin', asked: true };
    wrap(<PrintTechnologyPrompt firstRun wait={false} onChoose={vi.fn()} />);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_print_technology'));
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.queryByRole('region')).toBeNull();
  });
});

describe('what the choice hides', () => {
  const material = () => wrap(<PrintersHost>{(printers) => <FilamentView printerLink={link()} printers={printers} />}</PrintersHost>);

  it.each([['both', true], ['filament', false], ['resin', false]] as const)('offers the kind switch only for %s: %s', async (value, shown) => {
    stored = { value, asked: true };
    material();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('list_printers'));
    if (value !== 'both') await waitFor(() => expect(screen.queryByRole('group', { name: 'Art' })).toBeNull());
    await new Promise((r) => setTimeout(r, 0));
    expect(!!screen.queryByRole('group', { name: 'Art' })).toBe(shown);
  });

  it('lists only resin printers in the manager when printing with resin', async () => {
    stored = { value: 'resin', asked: true };
    wrap(<PrintersHost>{(printers) => <PrinterManagerView printers={printers} printerLink={link()} onMaterial={vi.fn()} />}</PrintersHost>);
    expect(await screen.findByRole('button', { name: /^Saturn/ })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Qidi/ })).toBeNull();
  });

  it('lists only filament printers in the manager when printing with filament', async () => {
    stored = { value: 'filament', asked: true };
    wrap(<PrintersHost>{(printers) => <PrinterManagerView printers={printers} printerLink={link()} onMaterial={vi.fn()} />}</PrintersHost>);
    expect(await screen.findByRole('button', { name: /^Qidi/ })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Saturn/ })).toBeNull();
  });

  it('shows both printers with both', async () => {
    stored = { value: 'both', asked: true };
    wrap(<PrintersHost>{(printers) => <PrinterManagerView printers={printers} printerLink={link()} onMaterial={vi.fn()} />}</PrintersHost>);
    expect(await screen.findByRole('button', { name: /^Saturn/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^Qidi/ })).toBeInTheDocument();
  });

  it('gives no spool check once resin only is known, and still gives one with both', async () => {
    const wrapper = ({ children }: { children: ReactNode }) => <PrintTechnologyProvider>{children}</PrintTechnologyProvider>;
    stored = { value: 'resin', asked: true };
    vi.mocked(invoke).mockImplementation(async (cmd) => cmd === 'get_print_technology' ? { ...stored } : [{ fileId: '7', status: 'ok', needs: [] }]);
    const resin = renderHook(() => useFilamentCheck(['7']), { wrapper });
    // The first answer may start a check before the choice is known; its result must not stay.
    await waitFor(() => expect(resin.result.current.checks?.size).toBe(0));
    resin.unmount();
    stored = { value: 'both', asked: true };
    const both = renderHook(() => useFilamentCheck(['7']), { wrapper });
    await waitFor(() => expect(both.result.current.checks?.get('7')?.status).toBe('ok'));
  });

  it('names the printer entry after the printers it lists', async () => {
    stored = { value: 'resin', asked: true };
    wrap(<Rail mainView="catalog" onMainViewChange={vi.fn()} trashCount={0} settingsOpen={false} onSettingsOpenChange={vi.fn()} />);
    expect(await screen.findByRole('button', { name: 'Printer Manager (nur Resin-Drucker)' })).toBeInTheDocument();
  });
});

function PrintersHost({ children }: { children: (printers: ReturnType<typeof usePrinters>) => ReactNode }) {
  const printers = usePrinters();
  return <>{children(printers)}</>;
}
