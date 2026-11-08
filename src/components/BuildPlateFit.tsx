import { useId } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatDimensions } from '../i18n/format';
import { plateFitForPrinters, usableModelSize, type PlateFitAxis } from '../lib/buildPlateFit';
import type { ModelFile, PrinterDetails } from '../types';
import { Icon } from './Icon';

type NamedPrinter = PrinterDetails & { id: string; name: string };

const EXCESS_KEY = { width: 'plateFitTooWide', depth: 'plateFitTooDeep', height: 'plateFitTooHigh' } as const satisfies Record<PlateFitAxis, string>;

// "Does it fit the print bed?" per printer of the Printer Manager. The
// verdict is always spelled out; the icon only repeats it.
export function BuildPlateFit({ model, printers, variant }: {
  model: Pick<ModelFile, 'dimensionsMm' | 'plateDimensions' | 'plateCount'>;
  printers: readonly NamedPrinter[];
  variant: 'compact' | 'comfort' | 'page';
}) {
  const t = useT();
  const { language } = useLanguage();
  const headingId = useId();
  const millimetres = new Intl.NumberFormat(language, { maximumFractionDigits: 1 });
  const excessText = (axis: PlateFitAxis, modelMm: number, bed: readonly number[]) =>
    t(EXCESS_KEY[axis]).replace('{mm}', millimetres.format(modelMm - bed[{ width: 0, depth: 1, height: 2 }[axis]]));
  const rows = plateFitForPrinters(model.dimensionsMm, printers, model.plateDimensions, model.plateCount);
  if (rows.length === 0) return null;
  const multi = (model.plateDimensions?.length ?? 0) > 1;
  const size = usableModelSize(model.dimensionsMm);

  const wrapper = variant === 'compact' ? 'px-4 pt-3.5 pb-3 border-t border-[var(--line)]'
    : variant === 'comfort' ? 'px-[18px] pt-3.5 pb-4 border-t border-[var(--line)]'
    : 'rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-5 py-4';
  const headingClass = variant === 'compact' ? 'text-compact-meta ui-label text-[var(--ink-3)]' : 'ui-label text-[var(--ink-3)]';
  const headingStyle = variant === 'comfort' ? { fontSize: 'var(--font-size-label)' } : undefined;

  return (
    <section aria-labelledby={headingId} className={wrapper}>
      <h3 id={headingId} className={`m-0 mb-1 ${headingClass}`} style={headingStyle}>{t('plateFitHeading')}</h3>
      <p className="m-0 mb-1.5 text-small text-[var(--ink-2)]">
        {multi ? t('plateFitPerPlate') : size ? t('plateFitModelSize').replace('{size}', () => formatDimensions(size, language)) : t('plateFitUnknown')}
      </p>
      <ul className="m-0 p-0 list-none flex flex-col gap-1">
        {rows.map(({ printer, bed, fit, plates }) => {
          const fits = fit.status === 'fits';
          const verdict = fit.status === 'unknown' ? t('plateFitUnknown') : t(fits ? 'plateFitFits' : 'plateFitTooLarge');
          const tone = fits ? 'text-[var(--good)]' : fit.status === 'tooLarge' ? 'text-[var(--crit)]' : 'text-[var(--ink-3)]';
          return (
            <li key={printer.id} className="text-small flex flex-wrap items-baseline gap-x-2">
              <span className={`inline-flex items-center gap-1 font-semibold ${tone}`}>
                {fit.status !== 'unknown' && <span aria-hidden="true"><Icon name={fits ? 'check' : 'close'} size={14} /></span>}
                {fits && multi ? t('plateFitAllPlates').replace('{count}', String(plates.length)) : verdict}
              </span>
              <span className="min-w-0 break-words text-[var(--ink)]">
                {printer.name} ({formatDimensions(bed, language).replace(/ mm$/, '')})
                {!multi && fit.status === 'tooLarge' && ': ' + fit.excess
                  .map(({ axis, modelMm }) => excessText(axis, modelMm, bed))
                  .join(', ')}
              </span>
              {multi && plates.filter(plate => plate.fit.status !== 'fits').map(plate => (
                <span key={plate.number} className="w-full min-w-0 break-words text-[var(--ink-2)]">
                  {t('plateFitPlate').replace('{number}', String(plate.number))}{plate.name ? ` “${plate.name}”` : ''}: {' '}
                  {plate.fit.status === 'tooLarge' ? plate.fit.excess.map(({ axis, modelMm }) =>
                    excessText(axis, modelMm, bed))
                    .join(', ') : t('plateFitUnknown')}
                </span>
              ))}
            </li>
          );
        })}
      </ul>
      <p className="m-0 mt-1.5 text-caption text-[var(--ink-3)]">{t('plateFitHint')}</p>
    </section>
  );
}
