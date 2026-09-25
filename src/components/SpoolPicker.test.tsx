import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { SpoolPicker } from './SpoolPicker';
import type { FilamentSpool } from '../types';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

const spool = (id: string, material: string, color: string, over: Partial<FilamentSpool> = {}): FilamentSpool => ({
  id, material, manufacturer: null, color, location: null, diameterMm: 1.75, originalWeightG: 1000,
  remainingWeightG: 612.4, price: null, imagePng: null, colorHex: '#8a8f94', homeLocation: null, unitId: null, slotIndex: null,
  ...over,
} as FilamentSpool);

describe('SpoolPicker', () => {
  it('opens a listbox and selects with the keyboard', () => {
    const onChange = vi.fn();
    render(
      <LanguageProvider>
        <SpoolPicker spools={[spool('1', 'PLA', 'Grau'), spool('2', 'PETG', 'Petrol')]} value="1" onChange={onChange} label="Spule" />
      </LanguageProvider>,
    );
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    const list = screen.getByRole('listbox');
    fireEvent.keyDown(list, { key: 'ArrowDown' });
    fireEvent.keyDown(list, { key: 'Enter' });
    expect(onChange).toHaveBeenCalledWith('2');
  });

  it('shows the remaining weight, so identical material/color spools stay distinguishable', () => {
    const onChange = vi.fn();
    render(
      <LanguageProvider>
        <SpoolPicker
          spools={[
            spool('1', 'PLA', 'Grau', { remainingWeightG: 612.4 }),
            spool('2', 'PLA', 'Grau', { remainingWeightG: 88 }),
          ]}
          value="1"
          onChange={onChange}
          label="Spule"
        />
      </LanguageProvider>,
    );
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    expect(screen.getByRole('option', { name: 'PLA · Grau · 612,4 g' })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: 'PLA · Grau · 88,0 g' })).toBeInTheDocument();
  });

  it('adds manufacturer and location to the label when available', () => {
    const onChange = vi.fn();
    render(
      <LanguageProvider>
        <SpoolPicker
          spools={[spool('1', 'PLA', 'Grau', { manufacturer: 'Prusament', location: 'Regal A' })]}
          value="1"
          onChange={onChange}
          label="Spule"
        />
      </LanguageProvider>,
    );
    expect(screen.getByRole('button', { name: /PLA · Grau · Prusament · Regal A · 612,4 g/ })).toBeInTheDocument();
  });

  it('does not bubble Escape to the surrounding dialog (only closes its own list)', () => {
    const onChange = vi.fn();
    const outerKeyDown = vi.fn();
    render(
      <LanguageProvider>
        <div onKeyDown={outerKeyDown}>
          <SpoolPicker spools={[spool('1', 'PLA', 'Grau')]} value="1" onChange={onChange} label="Spule" />
        </div>
      </LanguageProvider>,
    );
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    fireEvent.keyDown(screen.getByRole('listbox'), { key: 'Escape' });
    expect(outerKeyDown).not.toHaveBeenCalled();
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });

  it('exposes the active option via aria-activedescendant and moves it with ArrowDown', () => {
    const onChange = vi.fn();
    render(
      <LanguageProvider>
        <SpoolPicker spools={[spool('1', 'PLA', 'Grau'), spool('2', 'PETG', 'Petrol')]} value="1" onChange={onChange} label="Spule" />
      </LanguageProvider>,
    );
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    const list = screen.getByRole('listbox');
    const option1 = screen.getByRole('option', { name: /PLA/ });
    expect(list).toHaveAttribute('aria-activedescendant', option1.id);
    fireEvent.keyDown(list, { key: 'ArrowDown' });
    const option2 = screen.getByRole('option', { name: /PETG/ });
    expect(list).toHaveAttribute('aria-activedescendant', option2.id);
  });

  it('keeps the active option when the spool list re-renders with the same items while open', () => {
    const onChange = vi.fn();
    const first = [spool('1', 'PLA', 'Grau'), spool('2', 'PETG', 'Petrol')];
    const { rerender } = render(
      <LanguageProvider>
        <SpoolPicker spools={first} value="1" onChange={onChange} label="Spule" />
      </LanguageProvider>,
    );
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    const list = screen.getByRole('listbox');
    fireEvent.keyDown(list, { key: 'ArrowDown' });
    const activeBefore = list.getAttribute('aria-activedescendant');
    // Neues Array mit denselben Eintraegen (z.B. nach einem previewJob-Refresh
    // aus der umgebenden PrinterJobsDialog) - die Tastatur-Position darf
    // dabei NICHT zurueckspringen.
    const second = [spool('1', 'PLA', 'Grau'), spool('2', 'PETG', 'Petrol')];
    rerender(
      <LanguageProvider>
        <SpoolPicker spools={second} value="1" onChange={onChange} label="Spule" />
      </LanguageProvider>,
    );
    expect(screen.getByRole('listbox')).toHaveAttribute('aria-activedescendant', activeBefore);
  });

  it('moves the active option only once it actually disappears from a refreshed list', () => {
    const onChange = vi.fn();
    const first = [spool('1', 'PLA', 'Grau'), spool('2', 'PETG', 'Petrol')];
    const { rerender } = render(
      <LanguageProvider>
        <SpoolPicker spools={first} value="1" onChange={onChange} label="Spule" />
      </LanguageProvider>,
    );
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    const list = screen.getByRole('listbox');
    fireEvent.keyDown(list, { key: 'ArrowDown' }); // aktiv ist jetzt Spule 2 (PETG)
    rerender(
      <LanguageProvider>
        <SpoolPicker spools={[spool('1', 'PLA', 'Grau')]} value="1" onChange={onChange} label="Spule" />
      </LanguageProvider>,
    );
    const remaining = screen.getByRole('option', { name: /PLA/ });
    expect(screen.getByRole('listbox')).toHaveAttribute('aria-activedescendant', remaining.id);
  });

  it('does not close when a scroll event originates from inside the popup list itself', () => {
    // Regression: echte Browser loesen beim Oeffnen intern Scroll-Events aus
    // (z.B. scrollIntoView der aktiven Option, oder focus()-bedingtes
    // Scrollen) - jsdom tut das nicht von selbst, darum hier direkt
    // simuliert. Das darf das gerade geoeffnete Popup NICHT sofort wieder
    // schliessen.
    const onChange = vi.fn();
    render(
      <LanguageProvider>
        <SpoolPicker spools={[spool('1', 'PLA', 'Grau'), spool('2', 'PETG', 'Petrol')]} value="1" onChange={onChange} label="Spule" />
      </LanguageProvider>,
    );
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    const list = screen.getByRole('listbox');
    fireEvent.scroll(list);
    expect(screen.getByRole('listbox')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Spule/ })).toHaveAttribute('aria-expanded', 'true');
  });

  it('closes on a real scroll event from outside the popup (e.g. the page or an outer scroll container)', () => {
    const onChange = vi.fn();
    render(
      <LanguageProvider>
        <SpoolPicker spools={[spool('1', 'PLA', 'Grau')]} value="1" onChange={onChange} label="Spule" />
      </LanguageProvider>,
    );
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    expect(screen.getByRole('listbox')).toBeInTheDocument();
    fireEvent.scroll(window);
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });

  it('closes on window resize', () => {
    const onChange = vi.fn();
    render(
      <LanguageProvider>
        <SpoolPicker spools={[spool('1', 'PLA', 'Grau')]} value="1" onChange={onChange} label="Spule" />
      </LanguageProvider>,
    );
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    expect(screen.getByRole('listbox')).toBeInTheDocument();
    fireEvent(window, new Event('resize'));
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });

  it('opens on click and stays open (does not immediately close itself)', () => {
    // Regression aus commit 3365945: ein echter Klick oeffnete das Popup
    // nicht mehr, weil interne Scroll-/Fokus-Nebenwirkungen den eigenen
    // Schliessen-Listener ausgeloest haben.
    const onChange = vi.fn();
    render(
      <LanguageProvider>
        <SpoolPicker spools={[spool('1', 'PLA', 'Grau')]} value={null} onChange={onChange} label="Spule" placeholder="Spule wählen" />
      </LanguageProvider>,
    );
    const button = screen.getByRole('button', { name: /Spule/ });
    fireEvent.click(button);
    expect(button).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByRole('listbox')).toBeInTheDocument();
  });

  it('never offers a resin bottle (the printer link books filament only)', () => {
    render(
      <LanguageProvider>
        <SpoolPicker
          spools={[spool('1', 'PLA', 'Grau', { kind: 'filament' }), spool('2', 'Standard', 'Grau', { kind: 'resin', remainingWeightG: 640.5 })]}
          value={null}
          onChange={vi.fn()}
          label="Spule"
          placeholder="Spule wählen"
        />
      </LanguageProvider>,
    );
    fireEvent.click(screen.getByRole('button', { name: /Spule/ }));
    expect(screen.getAllByRole('option')).toHaveLength(1);
    expect(screen.queryByRole('option', { name: /Standard/ })).toBeNull();
  });
});
