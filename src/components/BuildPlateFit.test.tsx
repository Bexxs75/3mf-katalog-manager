import { render, screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import { BuildPlateFit } from './BuildPlateFit';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { makeModelFile, makePrinter } from '../test/factories';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';

it.each([['de', de], ['en', en], ['es', es], ['fr', fr]] as const)('renders multi-plate verdicts through %s keys', (language, t) => {
  localStorage.setItem('3mf-katalog-language', language);
  const model = makeModelFile({ dimensionsMm: [384.5, 445.2, 162], plateCount: 3, plateDimensions: [
    { number: 1, name: 'lithophane', dimensionsMm: [144, 108, 2.5] },
    { number: 2, name: 'photo frame', dimensionsMm: [28.5, 126, 162] },
    { number: 3, name: 'light guide plate', dimensionsMm: [108, 150, 1.4] },
  ] });
  render(<LanguageProvider><BuildPlateFit model={model} variant="compact" printers={[
    makePrinter({ id: 'fits', name: 'X1C' }), makePrinter({ id: 'short', name: 'Short', bedZMm: 150 }),
  ]} /></LanguageProvider>);
  const region = screen.getByRole('region', { name: t.plateFitHeading });
  expect(region).toHaveTextContent(t.plateFitPerPlate);
  expect(region).toHaveTextContent(t.plateFitAllPlates.replace('{count}', '3'));
  expect(region).toHaveTextContent(`${t.plateFitPlate.replace('{number}', '2')} “photo frame”`);
  expect(region).toHaveTextContent(t.plateFitTooHigh.replace('{mm}', '12'));
  expect(region).not.toHaveTextContent('384');
});

it.each([126, 126.00001])('rounds raw per-plate overshoots once (%s mm deep)', depth => {
  localStorage.setItem('3mf-katalog-language', 'de');
  const model = makeModelFile({ plateCount: 2, plateDimensions: [
    { number: 1, name: 'frame', dimensionsMm: [28.52, depth, 162.01] },
    { number: 2, name: null, dimensionsMm: [10, 10, 10] },
  ] });
  render(<LanguageProvider><BuildPlateFit model={model} variant="page" printers={[
    makePrinter({ bedXMm: 120, bedYMm: 120, bedZMm: 120 }),
  ]} /></LanguageProvider>);
  expect(screen.getByRole('region')).toHaveTextContent('6 mm zu tief, 42 mm zu hoch');
});

it('subtracts fractional bed sizes before rounding and localizes single-plate values', () => {
  localStorage.setItem('3mf-katalog-language', 'de');
  render(<LanguageProvider><BuildPlateFit model={makeModelFile({ dimensionsMm: [28.5, 126.04, 162.25] })} variant="page" printers={[
    makePrinter({ bedXMm: 119.96, bedYMm: 119.96, bedZMm: 120 }),
  ]} /></LanguageProvider>);
  expect(screen.getByRole('region')).toHaveTextContent('28,5 × 126 × 162,3 mm');
  expect(screen.getByRole('region')).toHaveTextContent('6,1 mm zu tief, 42,3 mm zu hoch');
});
