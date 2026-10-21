import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { SpoolColorDot, colorDotBackground } from './SpoolColorDot';

function renderDot(spool: Parameters<typeof SpoolColorDot>[0]['spool']) {
  localStorage.setItem('3mf-katalog-language', 'de');
  return render(<LanguageProvider><SpoolColorDot spool={spool} /></LanguageProvider>);
}

describe('colorDotBackground', () => {
  it('uses the plain color for one color', () => {
    expect(colorDotBackground(['#112233'])).toBe('#112233');
  });

  it('splits the dot into equal wedges, first color first', () => {
    expect(colorDotBackground(['#111111', '#222222'])).toBe('conic-gradient(#111111 0% 50%, #222222 50% 100%)');
    expect(colorDotBackground(['#111111', '#222222', '#333333', '#444444'])).toBe(
      'conic-gradient(#111111 0% 25%, #222222 25% 50%, #333333 50% 75%, #444444 75% 100%)',
    );
    expect(colorDotBackground(['#111111', '#222222', '#333333'])).toContain('#333333 66.67% 100%');
  });
});

describe('SpoolColorDot', () => {
  it('renders nothing without a color value', () => {
    const { container } = renderDot({ color: 'Rot', colorHex: null, colorHexes: [] });
    expect(container).toBeEmptyDOMElement();
  });

  it('ignores a value that is not #rrggbb', () => {
    const { container } = renderDot({ color: null, colorHex: 'red', colorHexes: [] });
    expect(container).toBeEmptyDOMElement();
  });

  it('names a single color by its color name', () => {
    renderDot({ color: 'Schwarz', colorHex: '#1a1a1a', colorHexes: [] });
    const dot = screen.getByRole('img', { name: 'Schwarz' });
    expect(dot).not.toHaveAttribute('data-multicolor');
  });

  it('stays decorative for a single color without a name', () => {
    renderDot({ color: null, colorHex: '#1a1a1a', colorHexes: [] });
    expect(screen.queryByRole('img')).toBeNull();
    expect(screen.getByTestId('spool-color-dot')).toHaveAttribute('aria-hidden', 'true');
  });

  it('says "multicolor" for several colors, with the name when there is one', () => {
    renderDot({ color: null, colorHex: '#111111', colorHexes: ['#111111', '#222222'] });
    expect(screen.getByRole('img', { name: 'mehrfarbig' })).toHaveAttribute('data-multicolor', 'true');
  });

  it('reads the list before the single value', () => {
    renderDot({ color: 'Duo', colorHex: '#111111', colorHexes: ['#111111', '#222222'] });
    expect(screen.getByRole('img', { name: 'Duo, mehrfarbig' })).toBeInTheDocument();
  });
});
