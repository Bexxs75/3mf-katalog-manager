import { useFormatCount, useLanguage, useT } from '../i18n/LanguageContext';
import { useOpenPlateOverview } from '../hooks/PlateOverviewContext';
import { formatLayerHeightMm, formatNozzleMm, formatPrintDuration } from '../i18n/format';
import { colorBucket, evaluate, FILTER_KEYS, type FilterKey, type PrintDataFilter } from '../lib/printDataFilter';
import { COLOR_LABEL_KEY, FILTER_LABEL_KEY } from '../lib/printFilterLabels';
import { printFactsOf } from '../lib/printData';
import type { ModelFile } from '../types';

/** Rows of the block: printer, values, colors with the "value missing" mark, and the plate overview entry. */
const ROW = { compact: { line: 15, last: 18, action: 18, gap: 2 }, comfort: { line: 20, last: 24, action: 22, gap: 2 } } as const;

/** Height of the block and the gap before it, the same for every card of a view. */
export function printFactsExtraHeight(comfort: boolean): number {
  const row = comfort ? ROW.comfort : ROW.compact;
  return 2 * row.line + row.last + row.action + 3 * row.gap + (comfort ? 8 : 6);
}

/** Names of the active filters this model has no value for (empty when it has all of them). */
export function missingFilterNames(model: Pick<ModelFile, 'printInfo' | 'printFacts'>, filter: PrintDataFilter, name: (key: FilterKey) => string): string[] {
  const { missing } = evaluate(model, filter);
  return FILTER_KEYS.filter(key => missing.includes(key)).map(name);
}

const MAX_DOTS = 6;

/**
 * What the active print data filter is about, on a card: target printer, time, layer height,
 * nozzle and colors, or "value missing" for the filters the file has no value for. A value that is
 * not known is left out or marked, never shown as 0.
 */
export function PrintFactsExtra({ model, filter, comfort }: { model: Pick<ModelFile, 'id' | 'printInfo' | 'printFacts'>; filter: PrintDataFilter; comfort: boolean }) {
  const t = useT();
  const formatCount = useFormatCount();
  const openPlateOverview = useOpenPlateOverview();
  const { language } = useLanguage();
  const facts = printFactsOf(model);
  const row = comfort ? ROW.comfort : ROW.compact;
  const missing = missingFilterNames(model, filter, key => t(FILTER_LABEL_KEY[key]) as string);

  const values: string[] = [];
  if (facts?.printSeconds !== undefined) {
    const time = formatPrintDuration(facts.printSeconds, language, t);
    values.push(facts.printSecondsComplete ? time : t('pfAtLeast').replace('{value}', time));
  }
  if (facts?.layerHeightMm !== undefined) values.push(formatLayerHeightMm(facts.layerHeightMm, language));
  if (facts && !facts.nozzleMixed && facts.nozzleMm !== undefined) values.push(`Ø ${formatNozzleMm(facts.nozzleMm, language)}`);

  const colors = facts?.colors ?? [];
  const missingText = missing.length > 0 ? t('pfMissingFor').replace('{what}', missing.join(', ')) : null;
  const rowStyle = (height: number) => ({ height, lineHeight: `${height}px` });
  return (
    <div data-print-facts className="flex flex-col shrink-0 overflow-hidden text-[var(--ink-3)] font-medium"
      style={{ height: printFactsExtraHeight(comfort) - (comfort ? 8 : 6), gap: row.gap, fontSize: 'var(--font-size-meta)' }}>
      <div className="truncate" style={rowStyle(row.line)} title={facts?.printerModel}>{facts?.printerModel ?? ''}</div>
      <div className="truncate tabular-nums" style={rowStyle(row.line)}>{values.join(' · ')}</div>
      <div className="flex items-center gap-1.5 min-w-0" style={{ height: row.last }}>
        {colors.slice(0, MAX_DOTS).map((color, index) => {
          const bucket = colorBucket(color);
          const name = bucket ? t(COLOR_LABEL_KEY[bucket]) as string : color;
          return <span key={`${color}${index}`} role="img" aria-label={name} title={name} className="flex-none inline-block w-3.5 h-3.5 rounded-full border border-[var(--line-strong)]" style={{ background: bucket ? color : 'transparent' }} />;
        })}
        {colors.length > MAX_DOTS && <span className="flex-none tabular-nums">+{colors.length - MAX_DOTS}</span>}
        {missingText && (
          <span title={missingText} className="min-w-0 truncate px-1.5 rounded border border-[var(--warn)] bg-[var(--warn-soft)] text-[var(--warn)] font-semibold" style={{ lineHeight: `${row.last - 4}px` }}>
            {missingText}
          </span>
        )}
      </div>
      <div style={rowStyle(row.action)}>
        {openPlateOverview && facts && facts.plateCount > 0 && (
          // The card selects on click and opens the detail page on double click; neither is meant here.
          <button type="button" onClick={event => { event.stopPropagation(); openPlateOverview(model.id); }} onDoubleClick={event => event.stopPropagation()}
            className="max-w-full truncate text-[var(--accent)] font-semibold cursor-pointer hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]"
            style={{ lineHeight: `${row.action}px` }}>
            {formatCount(t('poOpenCount'), facts.plateCount)}
          </button>
        )}
      </div>
    </div>
  );
}
