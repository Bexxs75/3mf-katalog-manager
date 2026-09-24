import { describe, expect, it, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { QueueFilamentSymbol } from './QueueFilamentSymbol';
import type { FilamentCheck } from '../types';

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
});

const renderSymbol = (check: FilamentCheck | undefined) =>
  render(<LanguageProvider><QueueFilamentSymbol check={check} /></LanguageProvider>);

describe('QueueFilamentSymbol', () => {
  it('renders nothing without a check', () => {
    const { container } = renderSymbol(undefined);
    expect(container).toBeEmptyDOMElement();
  });

  it.each([
    ['ok', '✓'],
    ['swap', '⇄'],
    ['short', '✗'],
    ['unknown', '?'],
    ['no_data', '–'],
  ] as const)('shows %s as %s', (status, symbol) => {
    renderSymbol({ fileId: '1', status, needs: [] });
    expect(screen.getByText(symbol)).toBeInTheDocument();
  });

  it('carries the tooltip text', () => {
    renderSymbol({
      fileId: '1',
      status: 'short',
      needs: [{ filamentType: 'PLA Silk', color: null, neededG: 31.5, status: 'short', missingG: 13.5, spools: [], possible: [] }],
    });
    expect(screen.getByText('✗')).toHaveAttribute('title', 'PLA Silk: es fehlen 13,5 g');
  });
});
