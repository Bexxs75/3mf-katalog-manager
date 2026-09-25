import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { PrinterJobsBanner } from './PrinterJobsBanner';
import type { PrinterJob } from '../types';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

const job = (id: string, grams: number | null, printerName = 'Sovol SV08'): PrinterJob => ({
  id, printerId: '1', printerName, fileName: 'a.gcode', outcome: 'completed', rawStatus: 'completed', endedAt: 1,
  printDurationS: 1, usedMm: 1, partialPercent: null, material: 'PLA', hasThumbnail: false, suggestedSpoolId: '100',
  grams, materialMismatch: false, modelMatch: null,
});

describe('PrinterJobsBanner', () => {
  it('is hidden without open jobs', () => {
    const { container } = render(<LanguageProvider><PrinterJobsBanner jobs={[]} onReview={vi.fn()} /></LanguageProvider>);
    expect(container).toBeEmptyDOMElement();
  });

  it('shows count, printers and total grams', () => {
    const onReview = vi.fn();
    render(<LanguageProvider><PrinterJobsBanner jobs={[job('1', 0.9), job('2', 55.1), job('3', 4)]} onReview={onReview} /></LanguageProvider>);
    expect(screen.getByText('3 neue Drucke warten auf Bestätigung')).toBeInTheDocument();
    expect(screen.getByText(/Sovol SV08 · zusammen 60,0 g/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Prüfen' }));
    expect(onReview).toHaveBeenCalled();
  });
});
