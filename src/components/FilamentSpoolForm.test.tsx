import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { FilamentSpoolForm } from './FilamentSpoolForm';
import type { FilamentSpool, SpoolKind } from '../types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

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
  colorHex: '#1a1a1a', homeLocation: 'Regal 2', unitId: 'u1', slotIndex: 0, kind: 'filament',
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
});
