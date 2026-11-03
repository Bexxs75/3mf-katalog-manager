import { useId, useState, type ReactNode } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import {
  formatLayerHeightMm, formatNozzleMm, formatPrintDuration, formatTemperatureC, formatUsageG, formatUsageM,
} from '../i18n/format';
import type { Translations } from '../i18n/types';
import { displayName } from '../lib/displayName';
import {
  mergeFilaments, sumPrintTime, sumWeightG, summarizePrintData,
  type PlateRow, type PrintDataSummary, type SharedValue,
} from '../lib/printData';
import type { FilamentUsage, ModelFile } from '../types';
import { useOpenPlateOverview } from '../hooks/PlateOverviewContext';
import { PlateSelector } from './PlateSelector';

type Variant = 'compact' | 'comfort' | 'page';

const NO_DATA_HINT: Partial<Record<PrintDataSummary['state'], keyof Translations>> = {
  no_slice_data: 'printDataNoDataHint',
  not_recognized: 'printDataNotRecognizedHint',
  unreadable: 'printDataUnreadableHint',
};

const focusClass = 'focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]';

/** A value the file does not give: a dash with a tooltip, and the same words for screen readers (a `title` reaches neither keyboard nor touch). */
function Missing({ reason }: { reason: string }) {
  return <span title={reason} className="text-[var(--ink-2)] font-normal">–<span className="sr-only"> {reason}</span></span>;
}

/**
 * One filament per line: the dot column is always reserved, so dots and names line up under each other
 * (also when a color is unknown). The dot border uses the muted ink token, which stays visible on both themes.
 */
function FilamentLine({ filament }: { filament: FilamentUsage }) {
  const { language } = useLanguage();
  return (
    <span className="flex items-center gap-2 text-left" data-filament-line>
      <span aria-hidden="true" className={`w-2.5 h-2.5 rounded-full flex-none ${filament.color ? 'border border-[var(--ink-3)]' : ''}`}
        style={filament.color ? { backgroundColor: filament.color.slice(0, 7) } : undefined} />
      <span>
        {filament.type}{' '}
        <span className="text-[var(--ink-3)]">{formatUsageG(filament.usedG, language)} · {formatUsageM(filament.usedM, language)}</span>
      </span>
    </span>
  );
}

/** `end` (panel) puts the block at the right edge of the value column, `start` (table) under its heading; lines always start at one left edge. */
function Filaments({ filaments, notFound, align }: { filaments: FilamentUsage[]; notFound: string; align: 'start' | 'end' }) {
  if (filaments.length === 0) return <Missing reason={notFound} />;
  return (
    <span className={`inline-flex flex-col gap-1 items-start ${align === 'end' ? 'text-left' : ''}`}>
      {filaments.map((filament, i) => <FilamentLine key={i} filament={filament} />)}
    </span>
  );
}

/** The card "Print data (from slicer)": the panel shows one selected plate at a time, the page lists all plates in a table. */
export function PrintDataCard({ model, variant, footer }: {
  model: Pick<ModelFile, 'sliceInfo' | 'printInfo'> & Partial<Pick<ModelFile, 'id'>>;
  variant: Variant;
  /** Page only: what belongs under the plate block (filament check, cost). */
  footer?: ReactNode;
}) {
  const t = useT();
  const { language } = useLanguage();
  const headingId = useId();
  const openPlateOverview = useOpenPlateOverview();
  const [selected, setSelected] = useState<number | null>(null);
  const summary = summarizePrintData(model);
  const { state, plates } = summary;
  const duration = (seconds: number) => formatPrintDuration(seconds, language, t);
  const notFound = t('printDataNotFound');
  const sharedReason = summary.settingsUnreadable ? t('printDataSettingsUnreadable') : notFound;

  const showTime = state === 'sliced' || state === 'partial';
  const showShared = summary.sharedKind !== 'none';
  const projectOnly = summary.sharedKind === 'project';
  const hintKey = NO_DATA_HINT[state];
  const current = plates.find(plate => plate.plateIndex === selected) ?? null;
  const scope: PlateRow[] = current ? [current] : plates;
  const allTime = summary.sum;
  const scopeTime = current ? sumPrintTime([current]) : allTime;
  const partialNote = (sum: { known: number; total: number }) => sum.known < sum.total
    ? t('printDataTotalPartial').replace('{k}', String(sum.known)).replace('{n}', String(sum.total)) : null;
  const geometryPlates = plates.map(plate => ({ number: plate.plateIndex, name: plate.name }));
  const plateLabel = (plate: PlateRow) => `${t('viewerPlate').replace('{n}', String(plate.plateIndex))}${plate.name ? ` · ${displayName(plate.name)}` : ''}`;

  // The overview reads the file itself, so it needs the id; the card of a bare summary has none.
  const overviewButton = openPlateOverview && model.id !== undefined && plates.length > 0 && (
    <button type="button" onClick={() => openPlateOverview(model.id as string)}
      className={`ml-auto px-2.5 py-0.5 rounded-[5px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-small font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] ${focusClass}`}>
      {t('poOpen')}
    </button>
  );

  const unreadText = variant === 'page'
    ? t(model.sliceInfo ? 'printDataUnreadKnown' : 'printDataUnreadPlain')
    : t('printDataUnreadPanel');

  const sharedValue = <T extends string | number>(value: SharedValue<T>, format: (value: T) => string): ReactNode => {
    if (value.kind === 'value') return format(value.value);
    if (value.kind === 'mixed') return t('printDataMixed');
    return <Missing reason={sharedReason} />;
  };
  const sharedEntries: { label: string; value: ReactNode }[] = [
    ...(variant === 'page'
      ? [{ label: t('printDataPrinter'), value: sharedValue(summary.shared.printerModel, v => v) },
         { label: t('printDataBedType'), value: sharedValue(summary.shared.bedType, v => v) }]
      : [{ label: t('printDataBedType'), value: sharedValue(summary.shared.bedType, v => v) },
         { label: t('printDataPrinter'), value: sharedValue(summary.shared.printerModel, v => v) }]),
    { label: t('printDataLayerHeight'), value: sharedValue(summary.shared.layerHeightMm, v => formatLayerHeightMm(v, language)) },
    { label: variant === 'page' ? t('printDataNozzleDiameter') : t('printDataNozzle'), value: sharedValue(summary.shared.nozzleMm, v => formatNozzleMm(v, language)) },
    ...(projectOnly ? [] : [
      { label: t('printDataBedTemp'), value: sharedValue(summary.shared.bedTempC, v => formatTemperatureC(v, language)) },
      { label: t('printDataNozzleTemp'), value: sharedValue(summary.shared.nozzleTempC, v => formatTemperatureC(v, language)) },
    ]),
  ];

  const chip = state === 'no_slice_data' && (
    <span className="inline-flex items-center h-[22px] px-[9px] rounded-full border border-[var(--line-strong)] bg-[var(--panel-2)] text-[var(--ink-2)] text-caption font-semibold">
      {t('printDataNoDataChip')}
    </span>
  );
  const hint = hintKey && <p className="m-0 text-small text-[var(--ink-3)] py-1.5 break-words">{t(hintKey) as string}</p>;
  const unreadBox = state === 'unread' && (
    <div className="rounded-md border border-dashed border-[var(--line-strong)] bg-[var(--panel-2)] px-3 py-2.5 my-2">
      <p className="m-0 text-small text-[var(--ink-2)]">{unreadText}</p>
    </div>
  );

  if (variant === 'page') {
    const headingClass = 'ui-label text-[var(--ink-3)]';
    return (
      <>
        <section aria-labelledby={headingId} className="rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-5 py-4">
          <div className="flex flex-wrap items-center gap-2 mb-3">
            <h3 id={headingId} className={`m-0 ${headingClass}`}>{t('printDataHeading')}</h3>
            {chip}
            {overviewButton}
          </div>
          {hint}
          {plates.length > 0 && (
            <>
              <PlateSelector plates={geometryPlates} selected={current ? current.plateIndex : null} onSelect={setSelected} />
              <div className="overflow-x-auto">
                <table className="w-full text-body border-collapse">
                  <caption className="sr-only">{t('printDataHeading')}</caption>
                  <thead>
                    <tr className="text-left">
                      <th scope="col" className="ui-label text-[var(--ink-3)] font-semibold py-2 pr-6 w-[28%]">{t('printDataColPlate')}</th>
                      {showTime && <th scope="col" className="ui-label text-[var(--ink-3)] font-semibold py-2 pr-8 w-[1%] whitespace-nowrap">{t('printDataTime')}</th>}
                      <th scope="col" className="ui-label text-[var(--ink-3)] font-semibold py-2 pr-8 w-[1%] whitespace-nowrap">{t('printDataWeight')}</th>
                      <th scope="col" className="ui-label text-[var(--ink-3)] font-semibold py-2">{t('metaMaterial')}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {plates.map(plate => (
                      <tr key={plate.plateIndex} className={`border-t border-[var(--line)] align-top ${current?.plateIndex === plate.plateIndex ? 'bg-[var(--panel-2)]' : ''}`}>
                        <th scope="row" className="text-left font-medium py-2 pr-3">
                          {plates.length > 1
                            ? <button type="button" aria-pressed={current?.plateIndex === plate.plateIndex} onClick={() => setSelected(plate.plateIndex)}
                                className={`text-left font-semibold hover:text-[var(--accent)] ${focusClass}`}>
                                {t('viewerPlate').replace('{n}', String(plate.plateIndex))}
                                {plate.name && <span className="block text-caption font-normal text-[var(--ink-3)]">{displayName(plate.name)}</span>}
                              </button>
                            : <>{t('viewerPlate').replace('{n}', String(plate.plateIndex))}{plate.name && <span className="block text-caption font-normal text-[var(--ink-3)]">{displayName(plate.name)}</span>}</>}
                        </th>
                        {showTime && <td className="py-2 pr-8 tabular-nums">{plate.printSeconds === null ? <Missing reason={notFound} /> : duration(plate.printSeconds)}</td>}
                        <td className="py-2 pr-8 tabular-nums">{plate.weightG === null ? <Missing reason={notFound} /> : formatUsageG(plate.weightG, language)}</td>
                        <td className="py-2 tabular-nums"><Filaments filaments={plate.filaments} notFound={notFound} align="start" /></td>
                      </tr>
                    ))}
                    {plates.length > 1 && (
                      <tr className="border-t border-[var(--line-strong)] font-semibold align-top">
                        <th scope="row" className="text-left py-2 pr-3">{t('printDataTotal')}</th>
                        {showTime && <td className="py-2 pr-8 tabular-nums">
                          {allTime.seconds === null ? <Missing reason={notFound} /> : <>
                            {duration(allTime.seconds)}
                            {partialNote(allTime) && <span className="block text-caption font-normal text-[var(--ink-3)]">{partialNote(allTime)}</span>}
                          </>}
                        </td>}
                        <td className="py-2 pr-8 tabular-nums">{sumWeightG(plates) === null ? <Missing reason={notFound} /> : formatUsageG(sumWeightG(plates) ?? 0, language)}</td>
                        <td />
                      </tr>
                    )}
                  </tbody>
                </table>
              </div>
            </>
          )}
          {unreadBox}
          {footer}
        </section>
        {showShared && (
          <section aria-labelledby={`${headingId}-shared`} className="rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-5 py-4">
            <h3 id={`${headingId}-shared`} className={`m-0 mb-3 ${headingClass}`}>{projectOnly ? t('printDataProjectSettings') : t('printDataSharedHeadingPage')}</h3>
            <dl className="m-0 grid gap-x-6 gap-y-2.5 [grid-template-columns:repeat(auto-fill,minmax(210px,1fr))]">
              {sharedEntries.map(entry => (
                <div key={entry.label} className="min-w-0">
                  <dt className="text-small text-[var(--ink-2)]">{entry.label}</dt>
                  <dd className="m-0 font-medium tabular-nums break-words">{entry.value}</dd>
                </div>
              ))}
            </dl>
          </section>
        )}
      </>
    );
  }

  // Panel (compact and comfortable): one selected plate, then the values shared by all plates.
  const wrapper = variant === 'compact' ? 'px-4 pt-3.5 pb-3 border-t border-[var(--line)]' : 'px-[18px] pt-3.5 pb-4 border-t border-[var(--line)]';
  const headingClass = variant === 'compact' ? 'text-compact-meta ui-label text-[var(--ink-3)]' : 'ui-label text-[var(--ink-3)]';
  const rowClass = variant === 'compact' ? 'flex items-baseline gap-3 py-1.5 border-b border-[var(--line)] text-small' : 'flex items-baseline gap-3 py-2 border-b border-[var(--line)]';
  // min-width instead of a fixed one: the bold total label must not wrap, the other labels keep the common edge.
  const labelClass = variant === 'compact' ? 'flex-none min-w-[108px] whitespace-nowrap text-[var(--ink-2)]' : 'flex-none min-w-[128px] whitespace-nowrap text-[var(--ink-2)]';
  const row = (label: string, value: ReactNode, sub?: string | null, strong = false) => (
    <div className={rowClass}>
      <span className={`${labelClass} ${strong ? 'font-semibold text-[var(--ink)]' : ''}`}>{label}</span>
      <span className={`flex-1 min-w-0 text-right font-medium tabular-nums break-words ${strong ? 'font-semibold' : ''}`}>
        {value}
        {sub && <span className="block text-caption font-normal text-[var(--ink-3)]">{sub}</span>}
      </span>
    </div>
  );
  const scopeWeight = sumWeightG(scope);
  return (
    <section aria-labelledby={headingId} className={wrapper} style={variant === 'comfort' ? { fontSize: 'var(--font-size-body)' } : undefined}>
      <div className="flex flex-wrap items-center gap-2 mb-2">
        <h3 id={headingId} className={`m-0 ${headingClass}`} style={variant === 'comfort' ? { fontSize: 'var(--font-size-label)' } : undefined}>{t('printDataHeading')}</h3>
        {chip}
        {overviewButton}
      </div>
      {hint}
      {plates.length > 0 && (
        <>
          <PlateSelector plates={geometryPlates} selected={current ? current.plateIndex : null} onSelect={setSelected} />
          {current && <div className="text-caption font-semibold text-[var(--ink-2)] pt-2.5 pb-0.5">{plateLabel(current)}</div>}
          {showTime && row(t('printDataTime'),
            scopeTime.seconds === null ? <Missing reason={notFound} /> : duration(scopeTime.seconds),
            current ? null : partialNote(scopeTime))}
          {row(t('printDataWeight'), scopeWeight === null ? <Missing reason={notFound} /> : formatUsageG(scopeWeight, language))}
          {row(t('metaMaterial'), <Filaments filaments={mergeFilaments(scope)} notFound={notFound} align="end" />)}
          {showTime && current && plates.length > 1 && row(t('printDataTotal'),
            allTime.seconds === null ? <Missing reason={notFound} /> : duration(allTime.seconds), partialNote(allTime), true)}
        </>
      )}
      {unreadBox}
      {showShared && (
        <>
          <div className="text-caption font-semibold text-[var(--ink-2)] pt-2.5 pb-0.5">{projectOnly ? t('printDataProjectSettings') : t('printDataSharedHeadingPanel')}</div>
          {sharedEntries.map(entry => <div key={entry.label}>{row(entry.label, entry.value)}</div>)}
        </>
      )}
    </section>
  );
}
