import { describe, expect, it, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { FilamentCheckSection } from './FilamentCheckSection';
import type { FilamentCheck, FilamentNeedCheck, FilamentSpoolUse } from '../types';

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
});

const spool = (over: Partial<FilamentSpoolUse> = {}): FilamentSpoolUse => ({
  spoolId: 's1', label: 'Bambu PLA · Rot', colorName: 'Rot', remainingG: 640, originalG: 1000, slot: null, location: 'Regal A', ...over,
});
const need = (over: Partial<FilamentNeedCheck>): FilamentNeedCheck => ({
  filamentType: 'PLA', color: '#C0392B', neededG: 208.46, status: 'ok', missingG: 0, spools: [spool()], possible: [], ...over,
});
const renderSection = (check: FilamentCheck | null, error = false) =>
  render(<LanguageProvider><FilamentCheckSection check={check} error={error} /></LanguageProvider>);

describe('FilamentCheckSection', () => {
  it('shows heading, overall status and an ok row with location', () => {
    renderSection({ fileId: '1', status: 'ok', needs: [need({})] });
    expect(screen.getByText('Reicht das Filament?')).toBeInTheDocument();
    expect(screen.getAllByText(/reicht$/)).toHaveLength(2); // Kopf-Chip + Zeilen-Chip
    expect(screen.getByText('PLA Rot')).toBeInTheDocument();
    expect(screen.getByText('208,5 g')).toBeInTheDocument();
    expect(screen.getByText('Bambu PLA · Rot')).toBeInTheDocument();
    expect(screen.getByText('Regal A')).toBeInTheDocument();
  });

  it('shows a fill bar for an ok row sized by remaining/original weight', () => {
    renderSection({ fileId: '1', status: 'ok', needs: [need({ spools: [spool({ remainingG: 640, originalG: 1000 })] })] });
    const bar = screen.getByRole('progressbar');
    expect(bar).toHaveAttribute('aria-valuenow', '64');
    expect(bar.firstChild).toHaveStyle({ width: '64%' });
  });

  it('shows a fill bar per spool for a short row', () => {
    renderSection({
      fileId: '1', status: 'short',
      needs: [need({ status: 'short', missingG: 13.46, spools: [spool({ remainingG: 18, originalG: 1000 })] })],
    });
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '2');
  });

  it('renders no fill bar without a known original weight', () => {
    renderSection({ fileId: '1', status: 'ok', needs: [need({ spools: [spool({ originalG: 0 })] })] });
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
  });

  it('shows the slot chip for a loaded spool', () => {
    renderSection({
      fileId: '1', status: 'ok',
      needs: [need({ spools: [spool({ slot: { printer: 'X1C', unit: 'AMS 1', slotNumber: 2 }, location: null })] })],
    });
    expect(screen.getByText('X1C · AMS 1 · Fach 2')).toBeInTheDocument();
  });

  it('shows the missing amount for short', () => {
    renderSection({ fileId: '1', status: 'short', needs: [need({ status: 'short', missingG: 13.46, spools: [spool({ remainingG: 18 })] })] });
    expect(screen.getByText('es fehlen 13,5 g')).toBeInTheDocument();
  });

  it('lists all spools for swap', () => {
    renderSection({
      fileId: '1', status: 'swap',
      needs: [need({ status: 'swap', spools: [spool({ spoolId: 'a', label: 'Sunlu PETG · Schwarz', remainingG: 38 }), spool({ spoolId: 'b', label: 'Eryone PETG · Schwarz', remainingG: 33 })] })],
    });
    expect(screen.getByText('2 Spulen zusammen 71 g')).toBeInTheDocument();
    expect(screen.getByText('Sunlu PETG · Schwarz')).toBeInTheDocument();
    expect(screen.getByText('Eryone PETG · Schwarz')).toBeInTheDocument();
  });

  it('shows possible matches for unknown', () => {
    renderSection({
      fileId: '1', status: 'unknown',
      needs: [need({ filamentType: 'TPU', status: 'unknown', spools: [], possible: [spool({ label: 'TPU 95A', colorName: null })] })],
    });
    expect(screen.getByText('Möglicher Treffer ohne Farbwert:')).toBeInTheDocument();
    expect(screen.getByText('TPU 95A')).toBeInTheDocument();
  });

  it('shows "no matching spool" for unknown without possible matches', () => {
    renderSection({ fileId: '1', status: 'unknown', needs: [need({ status: 'unknown', spools: [], possible: [] })] });
    expect(screen.getByText('Keine passende Spule')).toBeInTheDocument();
  });

  it('shows an error hint', () => {
    renderSection(null, true);
    expect(screen.getByText('Filament-Prüfung nicht möglich')).toBeInTheDocument();
  });

  it('renders nothing while loading or for no_data', () => {
    const { container } = renderSection(null);
    expect(container).toBeEmptyDOMElement();
    const second = renderSection({ fileId: '1', status: 'no_data', needs: [] });
    expect(second.container).toBeEmptyDOMElement();
  });
});
