import { useCallback, useRef, type ReactNode } from 'react';
import { formatLayerHeightMm, formatNozzleMm } from '../i18n/format';
import { useFormatCount, useLanguage, useT } from '../i18n/LanguageContext';
import { useDismissableMenu } from '../hooks/useDismissableMenu';
import {
  COLOR_SWATCH, FILTER_KEYS, type ColorId, type Facets, type FilterKey, type PrintDataFilter, type TimeBucket,
} from '../lib/printDataFilter';
import {
  COLOR_LABEL_KEY, FILTER_HINT_KEY, FILTER_LABEL_KEY, TIME_LABEL_KEY, choiceCount, withoutChoices,
} from '../lib/printFilterLabels';
import { Icon } from './Icon';

interface BarProps {
  filter: PrintDataFilter;
  onChange: (next: PrintDataFilter) => void;
  /** What the popovers offer; null while none is open and no filter is active. */
  facets: Facets | null;
  open: FilterKey | null;
  onOpenChange: (open: FilterKey | null) => void;
  /** Files the current filters show, for the live "matches" count. */
  hits: number;
}

const toggled = <T,>(list: readonly T[], value: T): T[] => list.includes(value) ? list.filter(entry => entry !== value) : [...list, value];

/** The five buttons of the print data filter, each with a popover of choices. */
export function PrintDataFilterBar(props: BarProps) {
  return <>{FILTER_KEYS.map(key => <FilterButton key={key} filterKey={key} {...props} />)}</>;
}

function FilterButton({ filterKey, filter, onChange, facets, open, onOpenChange, hits }: BarProps & { filterKey: FilterKey }) {
  const t = useT();
  const formatCount = useFormatCount();
  const wrapperRef = useRef<HTMLSpanElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const isOpen = open === filterKey;
  const close = useCallback(() => onOpenChange(null), [onOpenChange]);
  useDismissableMenu(isOpen, close, wrapperRef, triggerRef);
  const count = choiceCount(filter, filterKey);
  const label = t(FILTER_LABEL_KEY[filterKey]) as string;
  return (
    <span ref={wrapperRef} className="relative">
      <button ref={triggerRef} type="button" data-pf-button={filterKey} aria-haspopup="true" aria-expanded={isOpen}
        onClick={() => onOpenChange(isOpen ? null : filterKey)}
        className="inline-flex items-center gap-1.5 rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] px-2 py-1 text-small cursor-pointer hover:border-[var(--accent)]">
        {label}
        {count > 0 && <span className="min-w-[18px] px-1 rounded-full bg-[var(--accent)] text-[var(--bg)] text-caption font-semibold tabular-nums text-center">{count}</span>}
        <Icon name="chevron" size={12} />
      </button>
      {isOpen && facets && (
        <div role="group" aria-label={label} data-pf-popover={filterKey}
          className="absolute left-0 top-full mt-1 z-40 w-[290px] max-w-[calc(100vw-2rem)] max-h-[70vh] overflow-y-auto rounded-lg border border-[var(--line-strong)] bg-[var(--panel)] shadow-[var(--shadow)] p-3 flex flex-col gap-2">
          <h3 className="ui-label text-[var(--ink-3)]">{label}</h3>
          <Choices filterKey={filterKey} filter={filter} onChange={onChange} facets={facets} />
          <p className="text-caption text-[var(--ink-3)]">{t(FILTER_HINT_KEY[filterKey]) as string}</p>
          <div className="text-caption text-[var(--ink-3)] tabular-nums">{formatCount(t('pfNoData'), facets.missing[filterKey])}</div>
          <div className="flex items-center justify-between gap-2 pt-1 border-t border-[var(--line)]">
            <span aria-live="polite" className="text-small tabular-nums">{(t('pfHits') as string).replace('{count}', String(hits))}</span>
            <span className="flex gap-3">
              <button type="button" className="text-small text-[var(--accent)] cursor-pointer hover:underline disabled:opacity-50 disabled:cursor-default" disabled={count === 0}
                onClick={() => onChange(withoutChoices(filter, filterKey))}>{t('pfReset')}</button>
              <button type="button" className="text-small text-[var(--accent)] cursor-pointer hover:underline" onClick={() => { close(); triggerRef.current?.focus(); }}>{t('pfClose')}</button>
            </span>
          </div>
        </div>
      )}
    </span>
  );
}

function OptionRow({ checked, onToggle, name, count }: { checked: boolean; onToggle: () => void; name: ReactNode; count: number }) {
  return (
    <label className="flex items-center gap-2 text-small cursor-pointer">
      <input type="checkbox" checked={checked} onChange={onToggle} />
      <span className="grow min-w-0 break-words">{name}</span>
      <span className="text-[var(--ink-3)] tabular-nums">{count}</span>
    </label>
  );
}

function Choices({ filterKey, filter, onChange, facets }: { filterKey: FilterKey; filter: PrintDataFilter; onChange: (next: PrintDataFilter) => void; facets: Facets }) {
  const t = useT();
  const { language } = useLanguage();
  switch (filterKey) {
    case 'printer':
      return <>{facets.printers.map(entry => (
        <OptionRow key={entry.key} checked={filter.printers.includes(entry.key)} count={entry.count} name={entry.label}
          onToggle={() => onChange({ ...filter, printers: toggled(filter.printers, entry.key) })} />
      ))}</>;
    case 'nozzle':
      return <>{facets.nozzles.map(entry => (
        <OptionRow key={entry.key} checked={filter.nozzles.includes(entry.key)} count={entry.count} name={formatNozzleMm(entry.key, language)}
          onToggle={() => onChange({ ...filter, nozzles: toggled(filter.nozzles, entry.key) })} />
      ))}</>;
    case 'layer':
      return <>{facets.layers.map(entry => (
        <OptionRow key={entry.key} checked={filter.layers.includes(entry.key)} count={entry.count} name={formatLayerHeightMm(entry.key, language)}
          onToggle={() => onChange({ ...filter, layers: toggled(filter.layers, entry.key) })} />
      ))}</>;
    case 'time':
      return <>{facets.times.map(entry => (
        <OptionRow key={entry.key} checked={filter.times.includes(entry.key)} count={entry.count} name={t(TIME_LABEL_KEY[entry.key as TimeBucket]) as string}
          onToggle={() => onChange({ ...filter, times: toggled(filter.times, entry.key) })} />
      ))}</>;
    case 'color':
      return (
        <>
          <div className="flex flex-wrap gap-1.5">
            {facets.colors.map(entry => {
              const pressed = filter.colors.includes(entry.key);
              return (
                <button key={entry.key} type="button" aria-pressed={pressed}
                  onClick={() => onChange({ ...filter, colors: toggled(filter.colors, entry.key) })}
                  className={`inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-small cursor-pointer ${pressed ? 'border-[var(--accent)] bg-[var(--accent-soft)]' : 'border-[var(--line-strong)]'}`}>
                  <span aria-hidden="true" className="inline-block w-3.5 h-3.5 rounded-full border border-[var(--line-strong)]" style={{ background: COLOR_SWATCH[entry.key as ColorId] }} />
                  <span>{t(COLOR_LABEL_KEY[entry.key as ColorId]) as string}</span>
                  <span className="text-[var(--ink-3)] tabular-nums">{entry.count}</span>
                </button>
              );
            })}
          </div>
          <fieldset className="flex items-center gap-3 text-small">
            <legend className="sr-only">{t('pfColorModeLegend')}</legend>
            <span aria-hidden="true" className="ui-label text-[var(--ink-3)]">{t('pfColorModeLegend')}</span>
            {(['or', 'and'] as const).map(mode => (
              <label key={mode} className="inline-flex items-center gap-1 cursor-pointer">
                <input type="radio" name="pf-color-mode" checked={filter.colorMode === mode} onChange={() => onChange({ ...filter, colorMode: mode })} />
                {t(mode === 'or' ? 'pfModeOr' : 'pfModeAnd')}
              </label>
            ))}
          </fieldset>
          <p aria-live="polite" className="text-caption text-[var(--ink-3)]">{t(filter.colorMode === 'or' ? 'pfOrDesc' : 'pfAndDesc')}</p>
        </>
      );
  }
}
