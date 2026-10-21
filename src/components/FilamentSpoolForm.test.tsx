import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { FilamentSpoolForm } from './FilamentSpoolForm';
import type { FilamentSpool, SpoolKind } from '../types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

const drop = vi.hoisted(() => ({ handler: null as null | ((event: { payload: unknown }) => void) }));
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: (cb: (event: { payload: unknown }) => void) => {
      drop.handler = cb;
      return Promise.resolve(() => {});
    },
  }),
}));

beforeEach(() => { vi.mocked(invoke).mockReset(); vi.mocked(invoke).mockResolvedValue(undefined); });

const LOADED: FilamentSpool = {
  id: 's1', material: 'PLA', manufacturer: 'Bambu Lab', color: 'Galaxy Black', location: null,
  diameterMm: 1.75, originalWeightG: 1000, remainingWeightG: 600, price: null, imagePng: null,
  colorHex: '#1a1a1a', colorHexes: [], homeLocation: 'Regal 2', unitId: 'u1', slotIndex: 0, kind: 'filament',
};

function renderForm(editing: FilamentSpool | null, defaultKind?: SpoolKind) {
  localStorage.setItem('3mf-katalog-language', 'de');
  const onSaved = vi.fn();
  render(
    <LanguageProvider>
      <FilamentSpoolForm
        open
        editing={editing}
        knownLocations={[]}
        onClose={vi.fn()}
        onSaved={onSaved}
        defaultKind={defaultKind}
      />
    </LanguageProvider>,
  );
  return onSaved;
}

describe('FilamentSpoolForm', () => {
  it('reserves scrollbar width before validation adds content', () => {
    renderForm(null);
    expect(document.querySelector('.scrollbar-stable')).toHaveClass('overflow-y-scroll');
  });
  it('edits the home location of a spool that sits in a slot', () => {
    renderForm(LOADED);
    expect(screen.getByText('Stammplatz')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Regal 2')).toBeInTheDocument();
  });

  it('sends the chosen color value with the spool', async () => {
    const onSaved = renderForm(LOADED);
    fireEvent.click(screen.getByRole('radio', { name: '#27ae60' }));
    fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith(
      'update_filament_spool',
      expect.objectContaining({
        spool: expect.objectContaining({ colorHex: '#27ae60', location: 'Regal 2', unitId: 'u1', slotIndex: 0 }),
      }),
    );
  });

  it('shows the normal storage label for a spool in storage', () => {
    renderForm({ ...LOADED, unitId: null, slotIndex: null, homeLocation: null, location: 'Regal 1' });
    expect(screen.getByText('Lagerort')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Regal 1')).toBeInTheDocument();
  });

  it('creates a resin bottle with ml labels and without diameter', async () => {
    const onSaved = renderForm(null, 'resin');
    expect(screen.getByText('Inhalt (ml)')).toBeInTheDocument();
    expect(screen.getByText('Restmenge (ml)')).toBeInTheDocument();
    expect(screen.getByText('Anzahl Flaschen')).toBeInTheDocument();
    expect(screen.queryByText('Durchmesser (mm)')).toBeNull();
    fireEvent.change(screen.getByPlaceholderText('Material'), { target: { value: 'Standard' } });
    fireEvent.click(screen.getByRole('button', { name: 'Hinzufügen' }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith('add_filament_spool', expect.objectContaining({
      spool: expect.objectContaining({ kind: 'resin', material: 'Standard' }),
    }));
  });

  it('creates filament by default and shows spool wording', () => {
    renderForm(null);
    expect(screen.getByText('Anzahl Spulen')).toBeInTheDocument();
    expect(screen.getByText('Durchmesser (mm)')).toBeInTheDocument();
  });

  it('has no kind selector, neither when adding nor when editing', () => {
    renderForm(null, 'resin');
    expect(screen.queryByRole('button', { name: 'Filament' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Resin' })).toBeNull();
  });

  it('keeps the kind of an edited spool', async () => {
    const onSaved = renderForm({ ...LOADED, unitId: null, slotIndex: null, location: 'Regal 1' }, 'resin');
    expect(screen.getByText('Durchmesser (mm)')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith('update_filament_spool', expect.objectContaining({
      spool: expect.objectContaining({ kind: 'filament' }),
    }));
  });

  it('suggests resin materials in the resin view and filament materials otherwise', () => {
    renderForm(null, 'resin');
    fireEvent.focus(screen.getByPlaceholderText('Material'));
    expect(screen.getByText('ABS-like')).toBeInTheDocument();
    expect(screen.queryByText('PETG')).toBeNull();
  });

  it('suggests filament materials in the filament view', () => {
    renderForm(null, 'filament');
    fireEvent.focus(screen.getByPlaceholderText('Material'));
    expect(screen.getByText('PETG')).toBeInTheDocument();
    expect(screen.queryByText('ABS-like')).toBeNull();
  });

  it('is inert (unreachable) while closed, and not inert once opened', () => {
    localStorage.setItem('3mf-katalog-language', 'de');
    const { container, rerender } = render(
      <LanguageProvider>
        <FilamentSpoolForm open={false} editing={null} knownLocations={[]} onClose={vi.fn()} onSaved={vi.fn()} />
      </LanguageProvider>,
    );
    expect(container.querySelector('aside')).toHaveAttribute('inert');

    rerender(
      <LanguageProvider>
        <FilamentSpoolForm open editing={null} knownLocations={[]} onClose={vi.fn()} onSaved={vi.fn()} />
      </LanguageProvider>,
    );
    expect(container.querySelector('aside')).not.toHaveAttribute('inert');
  });

  function imageZone() {
    const button = screen.getByText('Bild hierher ziehen oder klicken').closest('button')!;
    button.getBoundingClientRect = () =>
      ({ left: 0, top: 0, right: 100, bottom: 100, width: 100, height: 100, x: 0, y: 0, toJSON: () => ({}) }) as DOMRect;
    return button;
  }

  it('shows unsupported dialog images without replacing the spool image', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => cmd === 'pick_and_read_image'
      ? Promise.reject({ message: 'imageUploadUnsupported', expected: true }) : Promise.resolve(undefined));
    renderForm(null);
    const button = imageZone();
    fireEvent.click(button);
    expect(await screen.findByText('Nur PNG-, JPG- oder WebP-Bilder werden unterstützt.')).toBeVisible();
    expect(button.querySelector('img')).toBeNull();
    expect(screen.queryByText('Problem melden')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Hinweis schließen' }));
    expect(screen.queryByText('Nur PNG-, JPG- oder WebP-Bilder werden unterstützt.')).toBeNull();
  });

  it('takes an image dropped onto the image field', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      Promise.resolve(cmd === 'read_dropped_image' ? 'QUJD' : undefined),
    );
    renderForm(null);
    const button = imageZone();
    act(() => drop.handler?.({ payload: { type: 'over', position: { x: 10, y: 10 } } }));
    expect(button).toHaveAttribute('data-drop-over', 'true');
    act(() => drop.handler?.({ payload: { type: 'drop', paths: ['/home/u/spule.png'], position: { x: 10, y: 10 } } }));
    await waitFor(() => expect(button.querySelector('img')).toHaveAttribute('src', 'data:image/png;base64,QUJD'));
    expect(invoke).toHaveBeenCalledWith('read_dropped_image', { path: '/home/u/spule.png' });
    expect(button).not.toHaveAttribute('data-drop-over');
  });

  it('keeps the form open while an image error is shown, until it is dismissed', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === 'read_dropped_image'
        ? Promise.reject('Bild ist zu groß (51.5 MB) - maximal 5 MB erlaubt')
        : Promise.resolve(undefined),
    );
    const onSaved = renderForm(null);
    imageZone();
    fireEvent.change(screen.getByPlaceholderText('Material'), { target: { value: 'PLA' } });
    act(() => drop.handler?.({ payload: { type: 'drop', paths: ['/home/u/zu-gross.png'], position: { x: 10, y: 10 } } }));
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Bild ist zu groß');
    fireEvent.click(screen.getByRole('button', { name: 'Hinzufügen' }));
    await act(async () => {});
    expect(invoke).not.toHaveBeenCalledWith('add_filament_spool', expect.anything());
    expect(onSaved).not.toHaveBeenCalled();
    expect(screen.getByRole('alert')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Hinweis schließen' }));
    expect(screen.queryByRole('alert')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Hinzufügen' }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
  });

  it('clears the image error when a valid image is dropped afterwards', async () => {
    renderForm(null);
    imageZone();
    act(() => drop.handler?.({ payload: { type: 'drop', paths: ['/a.txt'], position: { x: 10, y: 10 } } }));
    expect(screen.getByRole('alert')).toHaveTextContent('Nur PNG-, JPG- oder WebP-Bilder werden unterstützt.');
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      Promise.resolve(cmd === 'read_dropped_image' ? 'QUJD' : undefined),
    );
    act(() => drop.handler?.({ payload: { type: 'drop', paths: ['/b.png'], position: { x: 10, y: 10 } } }));
    await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
  });

  it('explains why several files are not taken', () => {
    renderForm(null);
    imageZone();
    act(() => drop.handler?.({ payload: { type: 'drop', paths: ['/a.png', '/b.png'], position: { x: 10, y: 10 } } }));
    expect(screen.getByText(/Bitte nur ein Bild hineinziehen\./)).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith('read_dropped_image', expect.anything());
  });

  it('offers "Report problem" for an unexpected save error but not for an expected one', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === 'update_filament_spool' ? Promise.reject({ message: 'x', expected: false }) : Promise.resolve(undefined),
    );
    renderForm(LOADED);
    fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
    await screen.findByText('Problem melden');

    vi.mocked(invoke).mockImplementation((cmd: string) =>
      cmd === 'update_filament_spool' ? Promise.reject({ message: 'x', expected: true }) : Promise.resolve(undefined),
    );
    fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
    await waitFor(() => expect(screen.queryByText('Problem melden')).not.toBeInTheDocument());
    expect(screen.getByText('x')).toBeInTheDocument();
  });
});

describe('numeric field validation', () => {
  const input = (label: string) => screen.getByText(label).parentElement!.querySelector('input')!;
  it.each([
    ['Durchmesser (mm)', '0'], ['Durchmesser (mm)', '-1'], ['Durchmesser (mm)', 'abc'],
    ['Durchmesser (mm)', '5.1'], ['Durchmesser (mm)', '1.75abc'],
    ['Preis', '-5'], ['Preis', 'abc'], ['Ursprungsgewicht (g)', '0'],
    ['Ursprungsgewicht (g)', '-1'], ['Restgewicht (g)', '-1'], ['Restgewicht (g)', '1001'],
  ])('rejects %s = %s at its field', async (label, value) => {
    const onSaved = renderForm(LOADED);
    const field = input(label);
    fireEvent.change(field, { target: { value } });
    fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
    expect(field).toHaveAttribute('aria-invalid', 'true');
    expect(field).toHaveClass('!border-[var(--crit)]');
    const message = document.getElementById(field.getAttribute('aria-describedby')!);
    expect(message).toHaveAttribute('role', 'alert');
    expect(field.parentElement).toContainElement(message);
    expect(field).toHaveFocus();
    await act(async () => {});
    expect(invoke).not.toHaveBeenCalledWith('update_filament_spool', expect.anything());
    expect(onSaved).not.toHaveBeenCalled();
  });

  it.each(['.', ','])('saves valid decimals using %s', async separator => {
    const onSaved = renderForm(LOADED);
    for (const [label, value] of [['Durchmesser (mm)', '1.75'], ['Preis', '24.90'], ['Ursprungsgewicht (g)', '1000.5'], ['Restgewicht (g)', '600.5']]) {
      fireEvent.change(input(label), { target: { value: value.replace('.', separator) } });
    }
    fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith('update_filament_spool', expect.objectContaining({ spool: expect.objectContaining({ diameterMm: 1.75, price: 24.9, originalWeightG: 1000.5, remainingWeightG: 600.5 }) }));
  });

  it('focuses the first invalid field and permits saving after correction', async () => {
    const onSaved = renderForm(LOADED);
    fireEvent.change(input('Durchmesser (mm)'), { target: { value: '0' } });
    fireEvent.change(input('Preis'), { target: { value: '-5' } });
    fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
    expect(input('Durchmesser (mm)')).toHaveFocus();
    fireEvent.change(input('Durchmesser (mm)'), { target: { value: '5' } });
    fireEvent.change(input('Preis'), { target: { value: '' } });
    fireEvent.change(input('Restgewicht (g)'), { target: { value: '0' } });
    fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(screen.queryByRole('alert')).toBeNull();
  });
});

it('keeps the stock preview numeric while a weight is cleared or invalid', () => {
  renderForm(LOADED);
  const remaining = screen.getByLabelText('Restgewicht (g)');
  for (const value of ['', 'abc']) {
    fireEvent.change(remaining, { target: { value } });
    expect(screen.getByRole('dialog')).not.toHaveTextContent('NaN');
  }
});

it('blocks creating a spool until invalid values are corrected', async () => {
  const onSaved = renderForm(null);
  fireEvent.change(screen.getByPlaceholderText('Material'), { target: { value: 'PLA' } });
  fireEvent.change(screen.getByLabelText('Durchmesser (mm)'), { target: { value: '0' } });
  fireEvent.click(screen.getByRole('button', { name: 'Hinzufügen' }));
  expect(screen.getByLabelText('Durchmesser (mm)')).toHaveFocus();
  expect(invoke).not.toHaveBeenCalledWith('add_filament_spool', expect.anything());
  expect(onSaved).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText('Durchmesser (mm)'), { target: { value: '1,75' } });
  fireEvent.click(screen.getByRole('button', { name: 'Hinzufügen' }));
  await waitFor(() => expect(onSaved).toHaveBeenCalled());
  expect(invoke).toHaveBeenCalledWith('add_filament_spool', expect.objectContaining({ spool: expect.objectContaining({ diameterMm: 1.75, price: null }) }));
});

it.each([NaN, Infinity])('sends a finite hidden diameter for a resin record with diameter %s', async diameterMm => {
  const onSaved = renderForm({ ...LOADED, kind: 'resin', diameterMm });
  expect(screen.queryByLabelText('Durchmesser (mm)')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
  await waitFor(() => expect(onSaved).toHaveBeenCalled());
  const call = vi.mocked(invoke).mock.calls.find(([command]) => command === 'update_filament_spool')!;
  const saved = (call[1] as { spool: FilamentSpool }).spool;
  expect(Number.isFinite(saved.diameterMm)).toBe(true);
  expect(saved.diameterMm).toBe(0);
});

it.each(['1e3', '1e0', '0x1', '12abc', '1,2.3', '+1'])('rejects non-decimal spool input %s in every numeric field', value => {
  renderForm(LOADED);
  const labels = ['Durchmesser (mm)', 'Preis', 'Ursprungsgewicht (g)', 'Restgewicht (g)'];
  for (const label of labels) fireEvent.change(screen.getByLabelText(label), { target: { value } });
  fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
  for (const label of labels) expect(screen.getByLabelText(label)).toHaveAttribute('aria-invalid', 'true');
  expect(invoke).not.toHaveBeenCalledWith('update_filament_spool', expect.anything());
});

it.each(['1,5', '1.5', '.5', ',5', ' 1.75 '])('accepts plain spool decimals %s with optional surrounding whitespace', async value => {
  const onSaved = renderForm(LOADED);
  for (const label of ['Durchmesser (mm)', 'Preis', 'Ursprungsgewicht (g)', 'Restgewicht (g)']) {
    fireEvent.change(screen.getByLabelText(label), { target: { value } });
  }
  fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
  await waitFor(() => expect(onSaved).toHaveBeenCalled());
  const numeric = Number(value.trim().replace(',', '.'));
  expect(invoke).toHaveBeenCalledWith('update_filament_spool', expect.objectContaining({ spool: expect.objectContaining({
    diameterMm: numeric, price: numeric, originalWeightG: Math.round(numeric * 10) / 10, remainingWeightG: Math.round(numeric * 10) / 10,
  }) }));
});

it.each(['0.01', '0,01', '0.049', '0.0001'])('rejects original weight %s that would round to zero', async value => {
  const onSaved = renderForm(LOADED);
  const original = screen.getByLabelText('Ursprungsgewicht (g)');
  fireEvent.change(original, { target: { value } });
  fireEvent.change(screen.getByLabelText('Restgewicht (g)'), { target: { value: '0' } });
  fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
  expect(original).toHaveAttribute('aria-invalid', 'true');
  expect(original).toHaveFocus();
  const message = document.getElementById(original.getAttribute('aria-describedby')!);
  expect(message).toHaveAttribute('role', 'alert');
  expect(message).toHaveTextContent('Nach Rundung');
  expect(message).toHaveTextContent('0,1');
  await act(async () => {});
  expect(invoke).not.toHaveBeenCalledWith('update_filament_spool', expect.anything());
  expect(onSaved).not.toHaveBeenCalled();
});

it.each(['0.05', '0,05', '0.1'])('preserves positive weight rounding at the boundary %s', async value => {
  const onSaved = renderForm(LOADED);
  fireEvent.change(screen.getByLabelText('Ursprungsgewicht (g)'), { target: { value } });
  fireEvent.change(screen.getByLabelText('Restgewicht (g)'), { target: { value } });
  fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
  await waitFor(() => expect(onSaved).toHaveBeenCalled());
  expect(invoke).toHaveBeenCalledWith('update_filament_spool', expect.objectContaining({ spool: expect.objectContaining({ originalWeightG: 0.1, remainingWeightG: 0.1 }) }));
});

function spoolWith(over: Partial<FilamentSpool>): FilamentSpool {
  return { ...LOADED, unitId: null, slotIndex: null, homeLocation: null, location: 'Regal 1', colorHex: null, ...over };
}

function renderFormWith(props: { editing?: FilamentSpool | null; knownSpools?: FilamentSpool[] }) {
  localStorage.setItem('3mf-katalog-language', 'de');
  const onSaved = vi.fn();
  render(
    <LanguageProvider>
      <FilamentSpoolForm
        open
        editing={props.editing ?? null}
        knownSpools={props.knownSpools}
        knownLocations={[]}
        onClose={vi.fn()}
        onSaved={onSaved}
      />
    </LanguageProvider>,
  );
  return onSaved;
}

function savedSpool(command: string): FilamentSpool {
  const call = vi.mocked(invoke).mock.calls.find(([name]) => name === command)!;
  return (call[1] as { spool: FilamentSpool }).spool;
}

describe('multi-color spools in the form', () => {
  it('starts with the one color picker and offers "+ Farbe"', () => {
    renderFormWith({});
    expect(screen.getByRole('group', { name: 'Farben (max. 4)' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '+ Farbe' })).toBeInTheDocument();
    expect(screen.queryByLabelText('Farbe 2')).toBeNull();
  });

  it('adds up to four colors in total and then hides the add button', () => {
    renderFormWith({});
    for (let i = 0; i < 3; i++) fireEvent.click(screen.getByRole('button', { name: '+ Farbe' }));
    expect(screen.getByLabelText('Farbe 2')).toBeInTheDocument();
    expect(screen.getByLabelText('Farbe 3')).toBeInTheDocument();
    expect(screen.getByLabelText('Farbe 4')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '+ Farbe' })).toBeNull();
  });

  it('focuses a new color field and returns focus to "+ Farbe" after removing one', () => {
    renderFormWith({});
    fireEvent.click(screen.getByRole('button', { name: '+ Farbe' }));
    expect(screen.getByLabelText('Farbe 2')).toHaveFocus();
    fireEvent.click(screen.getByRole('button', { name: 'Farbe 2 entfernen' }));
    expect(screen.queryByLabelText('Farbe 2')).toBeNull();
    expect(screen.getByRole('button', { name: '+ Farbe' })).toHaveFocus();
  });

  it('sends the list with the first color as colorHex', async () => {
    const onSaved = renderFormWith({});
    fireEvent.change(screen.getByPlaceholderText('Material'), { target: { value: 'PLA' } });
    fireEvent.click(screen.getByRole('radio', { name: '#c0392b' }));
    fireEvent.click(screen.getByRole('button', { name: '+ Farbe' }));
    fireEvent.change(screen.getByLabelText('Farbe 2'), { target: { value: '33AA66' } });
    fireEvent.click(screen.getByRole('button', { name: '+ Farbe' }));
    fireEvent.change(screen.getByLabelText('Farbe 3'), { target: { value: '#3366cc' } });
    fireEvent.click(screen.getByRole('button', { name: 'Hinzufügen' }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    const spool = savedSpool('add_filament_spool');
    expect(spool.colorHexes).toEqual(['#c0392b', '#33aa66', '#3366cc']);
    expect(spool.colorHex).toBe('#c0392b');
  });

  it('sends an empty list for a single color', async () => {
    const onSaved = renderFormWith({});
    fireEvent.change(screen.getByPlaceholderText('Material'), { target: { value: 'PLA' } });
    fireEvent.click(screen.getByRole('radio', { name: '#27ae60' }));
    fireEvent.click(screen.getByRole('button', { name: '+ Farbe' }));
    fireEvent.click(screen.getByRole('button', { name: 'Hinzufügen' }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    const spool = savedSpool('add_filament_spool');
    expect(spool.colorHexes).toEqual([]);
    expect(spool.colorHex).toBe('#27ae60');
  });

  it('drops a removed color from the list', async () => {
    const onSaved = renderFormWith({ editing: spoolWith({ colorHex: '#aa0000', colorHexes: ['#aa0000', '#00bb00', '#0000cc'] }) });
    expect(screen.getByLabelText('Farbe 2')).toHaveValue('#00bb00');
    expect(screen.getByLabelText('Farbe 3')).toHaveValue('#0000cc');
    fireEvent.click(screen.getByRole('button', { name: 'Farbe 2 entfernen' }));
    fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(savedSpool('update_filament_spool').colorHexes).toEqual(['#aa0000', '#0000cc']);
  });

  it('keeps duplicate colors', async () => {
    const onSaved = renderFormWith({ editing: spoolWith({ colorHex: '#aa0000', colorHexes: ['#aa0000', '#00bb00', '#aa0000'] }) });
    fireEvent.click(screen.getByRole('button', { name: 'Speichern' }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(savedSpool('update_filament_spool').colorHexes).toEqual(['#aa0000', '#00bb00', '#aa0000']);
  });

  it('shows a preview swatch per color and no preview for invalid text', () => {
    renderFormWith({ editing: spoolWith({ colorHex: '#aa0000', colorHexes: ['#aa0000', '#00bb00'] }) });
    const input = screen.getByLabelText('Farbe 2');
    const swatch = input.previousElementSibling as HTMLElement;
    expect(swatch.style.backgroundColor).not.toBe('');
    fireEvent.change(input, { target: { value: 'zzz' } });
    expect((input.previousElementSibling as HTMLElement).style.backgroundColor).toBe('');
  });

  it('blocks saving an invalid color and explains it', async () => {
    const onSaved = renderFormWith({});
    fireEvent.change(screen.getByPlaceholderText('Material'), { target: { value: 'PLA' } });
    fireEvent.click(screen.getByRole('button', { name: '+ Farbe' }));
    fireEvent.change(screen.getByLabelText('Farbe 2'), { target: { value: 'rot' } });
    fireEvent.click(screen.getByRole('button', { name: 'Hinzufügen' }));
    expect(screen.getByLabelText('Farbe 2')).toHaveAttribute('aria-invalid', 'true');
    expect(screen.getByLabelText('Farbe 2')).toHaveFocus();
    expect(screen.getByRole('alert')).toHaveTextContent('Farbwert');
    expect(invoke).not.toHaveBeenCalledWith('add_filament_spool', expect.anything());
    fireEvent.change(screen.getByLabelText('Farbe 2'), { target: { value: '#112233' } });
    fireEvent.click(screen.getByRole('button', { name: 'Hinzufügen' }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
  });

  it('ignores a color field that was added but left empty', async () => {
    const onSaved = renderFormWith({});
    fireEvent.change(screen.getByPlaceholderText('Material'), { target: { value: 'PLA' } });
    fireEvent.click(screen.getByRole('radio', { name: '#27ae60' }));
    fireEvent.click(screen.getByRole('button', { name: '+ Farbe' }));
    fireEvent.click(screen.getByRole('button', { name: 'Hinzufügen' }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(savedSpool('add_filament_spool').colorHexes).toEqual([]);
  });
});

describe('start weight of a new spool', () => {
  const known = [
    spoolWith({ id: '1', manufacturer: 'Polymaker', material: 'PLA', originalWeightG: 1000 }),
    spoolWith({ id: '2', manufacturer: 'Polymaker', material: 'PLA', originalWeightG: 750 }),
    spoolWith({ id: '3', manufacturer: 'Polymaker', material: 'PETG', originalWeightG: 500 }),
  ];
  const original = () => screen.getByLabelText('Ursprungsgewicht (g)') as HTMLInputElement;
  const remaining = () => screen.getByLabelText('Restgewicht (g)') as HTMLInputElement;
  const type = (placeholder: string, value: string) =>
    fireEvent.change(screen.getByPlaceholderText(placeholder), { target: { value } });

  it('starts with 1000 g and no hint', () => {
    renderFormWith({ knownSpools: known });
    expect(original().value).toBe('1000');
    expect(remaining().value).toBe('1000');
    expect(screen.queryByText(/zuletzt für/)).toBeNull();
  });

  it('takes the weight of the newest spool with this maker and material, for both fields', () => {
    renderFormWith({ knownSpools: known });
    type('Hersteller', 'Polymaker');
    type('Material', 'PLA');
    expect(original().value).toBe('750');
    expect(remaining().value).toBe('750');
    expect(screen.getByText('zuletzt für Polymaker PLA verwendet')).toBeInTheDocument();
  });

  it('falls back to the maker alone when the material is new', () => {
    renderFormWith({ knownSpools: known });
    type('Hersteller', 'Polymaker');
    type('Material', 'ASA');
    expect(original().value).toBe('500');
    expect(screen.getByText('zuletzt für Polymaker verwendet')).toBeInTheDocument();
  });

  it('falls back to 1000 g for an unknown maker and drops the hint again', () => {
    renderFormWith({ knownSpools: known });
    type('Hersteller', 'Polymaker');
    type('Material', 'PLA');
    expect(original().value).toBe('750');
    type('Hersteller', 'Unbekannt');
    expect(original().value).toBe('1000');
    expect(screen.queryByText(/zuletzt für/)).toBeNull();
  });

  it('does not overwrite a weight the user typed when maker or material change afterwards', () => {
    renderFormWith({ knownSpools: known });
    fireEvent.change(original(), { target: { value: '800' } });
    type('Hersteller', 'Polymaker');
    type('Material', 'PLA');
    expect(original().value).toBe('800');
    expect(remaining().value).toBe('750');
    expect(screen.queryByText(/zuletzt für/)).toBeNull();
  });

  it('keeps a typed rest weight but still follows the suggestion for the original weight', () => {
    renderFormWith({ knownSpools: known });
    fireEvent.change(remaining(), { target: { value: '300' } });
    type('Hersteller', 'Polymaker');
    type('Material', 'PLA');
    expect(remaining().value).toBe('300');
    expect(original().value).toBe('750');
  });

  it('saves the suggested weight and stays editable', async () => {
    const onSaved = renderFormWith({ knownSpools: known });
    type('Hersteller', 'Polymaker');
    type('Material', 'PLA');
    fireEvent.click(screen.getByRole('button', { name: 'Hinzufügen' }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(savedSpool('add_filament_spool')).toMatchObject({ originalWeightG: 750, remainingWeightG: 750 });
  });

  it('does not touch the weights of an edited spool', () => {
    renderFormWith({ knownSpools: known, editing: spoolWith({ manufacturer: 'Polymaker', material: 'PLA', originalWeightG: 1000, remainingWeightG: 400 }) });
    expect(original().value).toBe('1000');
    expect(remaining().value).toBe('400');
    expect(screen.queryByText(/zuletzt für/)).toBeNull();
  });
});
