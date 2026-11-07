import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import App from './App';
import { LanguageProviderWithDiagnostics } from './test/renderWithDiagnostics';
import { UiDensityProvider } from './hooks/UiDensityContext';
import { de } from './i18n/de';
import { makeModelFile, makeModelFileSummary, makePrintFacts } from './test/factories';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), convertFileSrc: (path: string) => path }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: vi.fn().mockResolvedValue(() => {}) }) }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn().mockResolvedValue(undefined), info: vi.fn().mockResolvedValue(undefined) }));
vi.mock('./components/BackgroundSnapshotRenderer', () => ({ BackgroundSnapshotRenderer: () => null }));
vi.mock('./components/ModelViewer', () => ({ ModelViewer: () => null }));

const x1c = makeModelFile({ id: '1', name: 'Benchy.3mf', printDataState: 'sliced',
  printFacts: makePrintFacts({ printerModel: 'Bambu Lab X1 Carbon', layerHeightMm: 0.2, nozzleMm: 0.4, plateCount: 1 }) });
const mk4 = makeModelFile({ id: '2', name: 'Vase.3mf', printDataState: 'sliced',
  printFacts: makePrintFacts({ printerModel: 'Prusa MK4S', layerHeightMm: 0.28, nozzleMm: 0.6, plateCount: 1 }) });
const unread = makeModelFile({ id: '3', name: 'Dragon.3mf' });
const stl = makeModelFile({ id: '4', name: 'Cube.stl', path: '/catalog/cube.stl', printDataState: 'none' });
const files = [x1c, mk4, unread, stl];

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem('3mf-katalog-base-dir', '/old');
  localStorage.setItem('3mf-katalog-setup-seen', '1');
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'get_app_version') return '0.15.0';
    if (cmd === 'has_step_preview' || cmd === 'is_preview_build' || cmd === 'get_printer_link_enabled') return false;
    if (cmd === 'register_existing_catalog_base_dir' || cmd === 'check_app_update' || cmd === 'get_last_printer_for_file') return null;
    if (cmd === 'list_file_summaries') return files.map(makeModelFileSummary);
    if (cmd === 'list_files_by_ids') return files.filter(m => (args as { ids: string[] }).ids.includes(m.id));
    if (cmd === 'list_all_file_tags') return {};
    if (cmd === 'read_plate_overview') return { plates: [{ plateIndex: 1, objectCount: 3 }] };
    return [];
  });
});

async function open() {
  await act(async () => { render(<LanguageProviderWithDiagnostics><UiDensityProvider><App /></UiDensityProvider></LanguageProviderWithDiagnostics>); });
  await screen.findAllByText('Benchy.3mf');
  return { main: screen.getByRole('main'), bar: screen.getByRole('region', { name: de.filterBarAria }) };
}

const choose = (bar: HTMLElement, filter: string, option: RegExp) => {
  const button = within(bar).getByRole('button', { name: new RegExp(`^${filter}`) });
  if (button.getAttribute('aria-expanded') !== 'true') {
    fireEvent.mouseDown(button);
    fireEvent.click(button);
  }
  fireEvent.click(within(screen.getByRole('group', { name: filter })).getByRole('checkbox', { name: option }));
};

it('filters by target printer, names what it holds back and shows it on request', async () => {
  const { main, bar } = await open();
  expect(within(bar).getByText(`4 von 4`)).toBeInTheDocument();
  expect(screen.queryByRole('status', { name: '' })?.textContent ?? '').not.toContain('ausgeblendet');

  choose(bar, de.pfFilterPrinter, /Prusa MK4S/);
  await waitFor(() => expect(main).not.toHaveTextContent('Benchy.3mf'));
  expect(main).toHaveTextContent('Vase.3mf');
  expect(main).not.toHaveTextContent('Dragon.3mf');
  expect(main).not.toHaveTextContent('Cube.stl');
  expect(within(bar).getByText('1 von 4')).toBeInTheDocument();
  // The card shows the facts only while a print data filter is active.
  expect(main.querySelectorAll('[data-print-facts]')).toHaveLength(1);
  expect(bar.querySelector('[data-filter-kind]')).toHaveTextContent(de.pfFilterPrinter);
  expect(bar).toHaveTextContent('Prusa MK4S');
  const notice = screen.getByText('2 Dateien ohne Druckdaten ausgeblendet').closest('[role="status"]') as HTMLElement;
  expect(notice).toHaveTextContent(de.pfHiddenDetail);

  const toggle = within(notice).getByRole('button', { name: de.pfShow });
  toggle.focus();
  fireEvent.click(toggle);
  await waitFor(() => expect(main).toHaveTextContent('Dragon.3mf'));
  expect(main).toHaveTextContent('Cube.stl');
  expect(main).toHaveTextContent(`${de.pfMissingFor.replace('{what}', de.pfFilterPrinter)}`);
  expect(main).not.toHaveTextContent('Benchy.3mf');
  expect(screen.getByText('2 Dateien ohne Druckdaten werden mit angezeigt')).toBeInTheDocument();
  // Same button, same focus: the switch does not lose the place.
  expect(within(notice).getByRole('button', { name: de.pfHide })).toBe(toggle);
  expect(toggle).toHaveFocus();
  expect(bar).toHaveTextContent(de.pfUnknownShown);
  expect(within(bar).getByText('3 von 4')).toBeInTheDocument();

  fireEvent.click(within(notice).getByRole('button', { name: de.pfHide }));
  await waitFor(() => expect(main).not.toHaveTextContent('Dragon.3mf'));
});

it('combines two filters with AND and lists only the files that have both values', async () => {
  const { main, bar } = await open();
  choose(bar, de.pfFilterPrinter, /Bambu Lab/);
  choose(bar, de.pfFilterLayer, /0,28 mm/);
  await waitFor(() => expect(main).toHaveTextContent(de.filterBarEmptyTitle));
  expect(main).not.toHaveTextContent('Vase.3mf');
});

it('moves the focus to the filter button after a chip is removed, and to the heading after the last one', async () => {
  const { bar } = await open();
  choose(bar, de.pfFilterPrinter, /Prusa MK4S/);
  choose(bar, de.pfFilterNozzle, /0,6 mm/);
  const chipOf = (kind: string) => within(bar).getByRole('button', { name: new RegExp(`^${de.chipRemove.replace('{label}', `${kind} .+`)}$`) });
  fireEvent.click(chipOf(de.pfFilterPrinter));
  expect(within(bar).getByRole('button', { name: new RegExp(`^${de.pfFilterPrinter}`) })).toHaveFocus();
  fireEvent.click(chipOf(de.pfFilterNozzle));
  expect(within(bar).getByText(de.filterBarHeading)).toHaveFocus();
  expect(bar.querySelectorAll('[data-filter-kind]')).toHaveLength(0);
});

it('resets every filter, shows the empty state with a reset, and puts the focus on the heading', async () => {
  const { main, bar } = await open();
  choose(bar, de.pfFilterPrinter, /Bambu Lab/);
  choose(bar, de.pfFilterLayer, /0,28 mm/);
  await waitFor(() => expect(main).toHaveTextContent(de.filterBarEmptyTitle));
  // The empty state has its own reset (the last one in the page); the bar has one too.
  const resets = within(main).getAllByRole('button', { name: de.filterBarClearAll });
  expect(resets).toHaveLength(2);
  fireEvent.click(resets[1]);
  await waitFor(() => expect(main).toHaveTextContent('Cube.stl'));
  expect(main).toHaveTextContent('Benchy.3mf');
  expect(bar.querySelectorAll('[data-filter-kind]')).toHaveLength(0);
  expect(within(bar).getByText(de.filterBarHeading)).toHaveFocus();
  expect(screen.queryByText(de.pfHiddenDetail)).toBeNull();
  expect(main.querySelector('[data-print-facts]')).toBeNull();
});

it('opens the plate overview from the card of a filtered file, reads the file for it, and closes it again', async () => {
  const { main, bar } = await open();
  choose(bar, de.pfFilterPrinter, /Prusa MK4S/);
  await waitFor(() => expect(main.querySelectorAll('[data-print-facts]')).toHaveLength(1));
  const entry = main.querySelector<HTMLButtonElement>('[data-print-facts] button')!;
  expect(entry).toHaveTextContent('Plattenübersicht (1 Platte)');
  fireEvent.click(entry);
  const dialog = await screen.findByRole('dialog', { name: new RegExp(`${de.poTitle}: Vase`) });
  expect(vi.mocked(invoke)).toHaveBeenCalledWith('read_plate_overview', { fileId: '2', withPreviews: true });
  await waitFor(() => expect(within(dialog).getAllByText('3').length).toBeGreaterThan(0));
  fireEvent.click(within(dialog).getByRole('button', { name: de.poClose }));
  expect(screen.queryByRole('dialog', { name: new RegExp(de.poTitle) })).toBeNull();
});

it('says how many held-back files are unread and leads to the block that reads them', async () => {
  const { bar } = await open();
  choose(bar, de.pfFilterPrinter, /Prusa MK4S/);
  const notice = (await screen.findByText('2 Dateien ohne Druckdaten ausgeblendet')).closest('[role="status"]') as HTMLElement;
  expect(notice).toHaveTextContent('Davon ist 1 noch nicht gelesen.');
  fireEvent.click(within(notice).getByRole('button', { name: de.pfUnreadReread }));
  expect(await screen.findByRole('tab', { name: de.settingsTabCatalog, selected: true })).toBeInTheDocument();
  expect(screen.getByRole('heading', { name: de.settingsSecPrintData })).toHaveFocus();
  expect(screen.getByRole('button', { name: de.printDataRereadAll })).toBeVisible();
});
