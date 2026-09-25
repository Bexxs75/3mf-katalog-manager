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
    const onSaved = renderForm(null);
    fireEvent.click(screen.getByRole('button', { name: 'Resin' }));
    expect(screen.getByText('Inhalt (ml)')).toBeInTheDocument();
    expect(screen.getByText('Restmenge (ml)')).toBeInTheDocument();
    expect(screen.queryByText('Durchmesser (mm)')).toBeNull();
    fireEvent.change(screen.getByPlaceholderText('Material'), { target: { value: 'Standard' } });
    fireEvent.click(screen.getByRole('button', { name: 'Hinzufügen' }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith('add_filament_spool', expect.objectContaining({
      spool: expect.objectContaining({ kind: 'resin', material: 'Standard' }),
    }));
  });

  it('preselects the kind of the current view for new entries', () => {
    renderForm(null, 'resin');
    expect(screen.getByRole('button', { name: 'Resin' })).toHaveAttribute('aria-pressed', 'true');
  });

  it('locks the kind of a spool that sits in a slot', () => {
    renderForm(LOADED);
    expect(screen.getByRole('button', { name: 'Resin' })).toBeDisabled();
    expect(screen.getByText('Die Art lässt sich erst ändern, wenn die Spule nicht im Drucker steckt.')).toBeInTheDocument();
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

  it('explains why several files are not taken', () => {
    renderForm(null);
    imageZone();
    act(() => drop.handler?.({ payload: { type: 'drop', paths: ['/a.png', '/b.png'], position: { x: 10, y: 10 } } }));
    expect(screen.getByText(/Bitte nur ein Bild hineinziehen\./)).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith('read_dropped_image', expect.anything());
  });
});
