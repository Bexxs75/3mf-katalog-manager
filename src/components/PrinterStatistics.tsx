import { useEffect, useId, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { useFormatCount, useLanguage, useT } from '../i18n/LanguageContext';
import { formatPrintDuration } from '../i18n/format';
import type { Translations } from '../i18n/types';
import { ErrorText } from '../diagnostics/ErrorText';
import { toAppError, type AppError } from '../lib/errors';
import { getPrintStatistics } from '../lib/api/printers';
import {
  byDays, byMonth, inRange, materialGrams, reasonsBreakdown, totals, RANGE_MONTHS,
  type Counts, type StatRange, type StatOutcome,
} from '../lib/printStatistics';
import type { Printer, PrintStatistics } from '../types';
import { Icon } from './Icon';
import { SegmentedControl } from './SegmentedControl';
import { useFailureReasonText } from './PrintFailureReasonField';

type View = 'chart' | 'table';
const ALL = 'all';

/** Good, warn and crit are told apart by a pattern as well as by colour. */
const FILL: Record<StatOutcome, CSSProperties> = {
  succeeded: { background: 'var(--good)' },
  cancelled: { background: 'repeating-linear-gradient(45deg, var(--warn) 0 3px, var(--warn-soft) 3px 6px)' },
  failed: { background: 'repeating-linear-gradient(90deg, var(--crit) 0 3px, var(--crit-soft) 3px 6px)' },
};
const OUTCOMES: readonly StatOutcome[] = ['succeeded', 'cancelled', 'failed'];
const OUTCOME_KEY = { succeeded: 'pfResultSucceeded', cancelled: 'pfResultCancelled', failed: 'pfResultFailed' } as const satisfies Record<StatOutcome, keyof Translations>;
const GROUP_KEY = {
  succeeded: 'pstGroupSucceeded', cancelled: 'pstGroupCancelled', failed: 'pstGroupFailed',
  printer_error: 'pstGroupPrinterError', interrupted: 'pstGroupInterrupted', other: 'pstGroupOther',
} as const;

function Heading({ children }: { children: ReactNode }) {
  return <h3 className="text-body font-bold">{children}</h3>;
}

function Note({ title, children }: { title?: string; children: ReactNode }) {
  return <div className="rounded-lg border border-[var(--line)] bg-[var(--panel-2)] p-3">
    {title && <b className="block">{title}</b>}<p className="text-[var(--ink-2)]">{children}</p>
  </div>;
}

/** A horizontal bar with its name and number; the number is text, never only the bar length. */
function HBar({ label, value, max, text }: { label: string; value: number; max: number; text: string }) {
  return <li className="grid grid-cols-[minmax(0,12rem)_minmax(0,1fr)_auto] max-[639px]:grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 gap-y-1">
    <span className="break-words">{label}</span>
    <span aria-hidden="true" className="max-[639px]:order-3 max-[639px]:col-span-2 h-2.5 rounded bg-[var(--panel-2)] overflow-hidden">
      <span className="block h-full rounded bg-[var(--accent)]" style={{ width: `${max > 0 ? Math.max(2, (100 * value) / max) : 0}%` }} />
    </span>
    <span className="tabular-nums text-right whitespace-nowrap">{text}</span>
  </li>;
}

export function PrinterStatistics({ printers, printerId, refreshKey }: {
  printers: Printer[]; printerId?: string; refreshKey?: unknown;
}) {
  const t = useT();
  const { language } = useLanguage();
  const formatCount = useFormatCount();
  const reasonText = useFailureReasonText();
  const isWeb = import.meta.env.VITE_TARGET === 'web';
  const headingId = useId();
  const selectId = useId();
  const [scope, setScope] = useState<string>(ALL);
  const [range, setRange] = useState<StatRange>('12m');
  const [view, setView] = useState<View>('chart');
  const [data, setData] = useState<PrintStatistics | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  const firstRun = useRef(true);
  // Picking a printer in the list takes the statistics along; "All printers" stays a choice in this card.
  useEffect(() => {
    if (firstRun.current) { firstRun.current = false; return; }
    if (printerId) setScope(printerId);
  }, [printerId]);
  const known = printers.some(p => p.id === scope);
  const effective = scope === ALL || known ? scope : ALL;
  useEffect(() => {
    let active = true;
    setError(null);
    getPrintStatistics(effective === ALL ? undefined : effective)
      .then(result => { if (active) { setData(result); setNow(Math.floor(Date.now() / 1000)); } })
      .catch(e => { if (active) { setError(toAppError(e)); setData(null); } });
    return () => { active = false; };
  }, [effective, refreshKey]);

  const listedIds = useMemo(() => new Set(printers.map(p => p.id)), [printers]);
  // Printers of a hidden kind are not listed, so their prints stay out of "All printers" as well.
  const all = useMemo(() => (data?.rows ?? []).filter(r => listedIds.has(r.printerId)), [data, listedIds]);
  const rows = useMemo(() => inRange(all, range, now), [all, range, now]);
  const sum = useMemo(() => totals(rows), [rows]);
  const months = useMemo(() => range === '30d' ? null : byMonth(rows, now, undefined, range === '12m' ? RANGE_MONTHS : null), [rows, range, now]);
  const blocks = useMemo(() => range === '30d' ? byDays(rows, now, undefined) : null, [rows, range, now]);
  const reasons = useMemo(() => reasonsBreakdown(rows), [rows]);
  const materials = useMemo(() => materialGrams(rows), [rows]);

  const number = useMemo(() => new Intl.NumberFormat(language, { maximumFractionDigits: 0 }), [language]);
  const gramsText = (g: number) => `${number.format(g)} g`;
  const duration = (s: number) => formatPrintDuration(s, language, t);
  const monthName = useMemo(() => new Intl.DateTimeFormat(language, { month: 'short', year: '2-digit', timeZone: 'UTC' }), [language]);
  const dayName = useMemo(() => new Intl.DateTimeFormat(language, { day: 'numeric', month: 'numeric', timeZone: 'UTC' }), [language]);
  const dateName = useMemo(() => new Intl.DateTimeFormat(language, { dateStyle: 'medium' }), [language]);
  const items: { key: string; label: string; counts: Counts }[] = months
    ? months.map(b => ({ key: b.key, label: monthName.format(Date.UTC(b.year, b.month - 1, 1)), counts: b }))
    : (blocks ?? []).map(b => ({
      key: String(b.startDay), counts: b,
      label: `${dayName.format(b.startDay * 86_400_000)}–${dayName.format(b.endDay * 86_400_000)}`,
    }));
  const peak = Math.max(1, ...items.map(i => i.counts.total));
  const bucketText = (i: typeof items[number]) => t('pstBucketSummary').replace('{label}', () => i.label)
    .replace('{succeeded}', String(i.counts.succeeded)).replace('{cancelled}', String(i.counts.cancelled)).replace('{failed}', String(i.counts.failed));
  const rangeName = { '30d': t('pstRange30'), '12m': t('pstRange12'), all: t('pstRangeAll') }[range];
  const scopeName = effective === ALL ? t('pstAllPrinters') : printers.find(p => p.id === effective)?.name ?? '';
  const share = (count: number, total: number) => t('pstShare').replace('{count}', String(count)).replace('{percent}', number.format((100 * count) / Math.max(1, total)));
  const rangeOptions: { value: StatRange; label: string }[] = [
    { value: '30d', label: t('pstRange30') }, { value: '12m', label: t('pstRange12') }, { value: 'all', label: t('pstRangeAll') },
  ];
  const ready = data !== null;
  const since = data?.firstConnectedAt != null ? dateName.format(data.firstConnectedAt * 1000) : null;
  const unitLabel = months ? t('pstPerMonth') : t('pstPerBlock');

  return <section aria-labelledby={headingId} aria-busy={!ready && !error} className="min-w-0 rounded-lg border border-[var(--line)] bg-[var(--panel)] overflow-hidden">
    <h2 id={headingId} className="px-[18px] pt-3.5 text-title font-bold">{t('pstTitle')}</h2>
    <div className="p-3.5 flex flex-col gap-3">
      <div className="flex flex-wrap items-end gap-x-6 gap-y-3">
        <div className="flex flex-col gap-1">
          <label htmlFor={selectId} className="text-small text-[var(--ink-2)]">{t('pstPrinter')}</label>
          <select id={selectId} value={effective} onChange={e => setScope(e.target.value)}
            className="min-w-[14rem] max-w-full px-2 py-1.5 rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-small">
            <option value={ALL}>{t('pstAllPrinters')}</option>
            {printers.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
          </select>
        </div>
        <div className="flex flex-col gap-1">
          <span aria-hidden="true" className="text-small text-[var(--ink-2)]">{t('pstRange')}</span>
          <SegmentedControl label={t('pstRange')} value={range} onChange={setRange} options={rangeOptions} />
        </div>
      </div>
      {/* Spoken after every change of printer, period or data; the card itself is not announced. */}
      <p role="status" className="sr-only">{ready ? t('pstAnnounce').replace('{printer}', () => scopeName).replace('{range}', rangeName).replace('{count}', String(sum.total)) : ''}</p>
      {isWeb && <Note>{t('pstWeb')}</Note>}
      {since && <p className="text-[var(--ink-3)]">{t('pstSince').replace('{date}', since)}</p>}
      {data?.truncated && <Note title={t('pstTruncatedTitle')}>{t('pstTruncatedBody')}</Note>}
      {error && <div role="alert" className="text-[var(--crit)]"><b>{t('pstLoadFailed')}</b> <ErrorText error={error} /></div>}
      {!ready && !error && <p className="text-[var(--ink-3)]">{t('pstLoading')}</p>}
      {ready && all.length === 0 && <div className="rounded-lg border border-[var(--line)] bg-[var(--panel-2)] flex flex-col items-center gap-2 px-3.5 py-8 text-center">
        <Icon name="printer" size={24} /><b>{t('pstEmptyTitle')}</b>
        <p className="max-w-[46ch] text-[var(--ink-3)]">{t(isWeb ? 'pstEmptyBodyWeb' : 'pstEmptyBody')}</p>
      </div>}
      {ready && all.length > 0 && rows.length === 0 && <p className="text-[var(--ink-3)]">{t('pstEmptyRange')}</p>}
      {ready && rows.length > 0 && <>
        <div className="grid grid-cols-1 min-[900px]:grid-cols-3 gap-3">
          <div className="rounded-lg border border-[var(--line)] p-3 flex flex-col gap-1">
            <span className="ui-label text-[var(--ink-3)]">{t('pstKpiPrints')}</span>
            <span className="text-heading font-bold tabular-nums">{number.format(sum.total)}</span>
            <span className="text-[var(--ink-2)]">{t('pstKpiBreakdown').replace('{succeeded}', String(sum.succeeded)).replace('{cancelled}', String(sum.cancelled)).replace('{failed}', String(sum.failed))}</span>
            <span className="text-[var(--ink-3)]">{t('pstKpiTime').replace('{time}', duration(sum.printSeconds))}</span>
          </div>
          <div className="rounded-lg border border-[var(--line)] p-3 flex flex-col gap-1">
            <span className="ui-label text-[var(--ink-3)]">{t('pstKpiRate')}</span>
            <span className="text-heading font-bold tabular-nums">{sum.successRate} %</span>
            <span className="text-[var(--ink-2)]">{t('pstKpiRateOf').replace('{succeeded}', String(sum.succeeded)).replace('{total}', String(sum.total))}</span>
            <span className="text-[var(--ink-3)]">{t('pstKpiRateNote')}</span>
          </div>
          <div className="rounded-lg border border-[var(--line)] p-3 flex flex-col gap-1">
            <span className="ui-label text-[var(--ink-3)]">{t('pstKpiFilament')}</span>
            <span className="text-heading font-bold tabular-nums">{gramsText(sum.grams)}</span>
            <span className="text-[var(--ink-2)]">{t('pstKpiConfirmed')}</span>
            <span className="text-[var(--ink-3)]">{formatCount(t('pstWithoutGrams'), sum.withoutGrams)}</span>
          </div>
        </div>

        <div className="flex flex-wrap items-center justify-between gap-2">
          <Heading>{unitLabel}</Heading>
          <SegmentedControl label={t('pstView')} value={view} onChange={setView}
            options={[{ value: 'chart', label: t('pstViewChart') }, { value: 'table', label: t('pstViewTable') }]} />
        </div>
        {view === 'chart' ? <>
          <div role="img" aria-label={t('pstChartLabel').replace('{summary}', () => items.map(bucketText).join('; '))} className="overflow-x-auto">
            <div className="flex items-end gap-1.5 min-w-max sm:min-w-0 h-40">
              {items.map(i => <div key={i.key} title={`${i.label}: ${i.counts.total}`} className="flex-1 min-w-[2.25rem] h-full flex flex-col justify-end items-stretch gap-0.5">
                <span className="text-caption tabular-nums text-center text-[var(--ink-2)]">{i.counts.total || ''}</span>
                <div className="flex flex-col-reverse" style={{ height: `${(100 * i.counts.total) / peak}%`, maxHeight: 'calc(100% - 1.25rem)' }}>
                  {OUTCOMES.map(o => i.counts[o] > 0 && <div key={o} style={{ ...FILL[o], flexGrow: i.counts[o], flexBasis: 0 }} />)}
                </div>
              </div>)}
            </div>
            <div aria-hidden="true" className="flex gap-1.5 mt-1 min-w-max sm:min-w-0">
              {items.map(i => <span key={i.key} className="flex-1 min-w-[2.25rem] text-center text-caption text-[var(--ink-3)] whitespace-nowrap">{i.label}</span>)}
            </div>
          </div>
          <ul className="flex flex-wrap gap-x-5 gap-y-1 text-small">
            {OUTCOMES.map(o => <li key={o} className="flex items-center gap-1.5">
              <span aria-hidden="true" className="inline-block w-4 h-3 rounded-sm" style={FILL[o]} />
              {t(OUTCOME_KEY[o])}{o !== 'succeeded' && <span className="text-[var(--ink-3)]"> ({t(o === 'cancelled' ? 'pstPatternDiagonal' : 'pstPatternVertical')})</span>}
            </li>)}
          </ul>
        </> : <div role="region" aria-label={unitLabel} tabIndex={0} className="overflow-auto max-h-[28rem]">
          <table className="w-full text-left text-small">
            <caption className="sr-only">{unitLabel}</caption>
            <thead className="ui-label text-[var(--ink-3)]"><tr>
              <th scope="col" className="p-2 border-b border-[var(--line)]">{t(months ? 'pstColMonth' : 'pstColPeriod')}</th>
              {OUTCOMES.map(o => <th scope="col" key={o} className="p-2 border-b border-[var(--line)] text-right">{t(OUTCOME_KEY[o])}</th>)}
              <th scope="col" className="p-2 border-b border-[var(--line)] text-right">{t('pstColTotal')}</th>
            </tr></thead>
            <tbody>{items.map(i => <tr key={i.key}>
              <th scope="row" className="p-2 font-normal whitespace-nowrap">{i.label}</th>
              {OUTCOMES.map(o => <td key={o} className="p-2 text-right tabular-nums">{i.counts[o]}</td>)}
              <td className="p-2 text-right tabular-nums font-bold">{i.counts.total}</td>
            </tr>)}</tbody>
          </table>
        </div>}

        <div className="grid grid-cols-1 min-[1100px]:grid-cols-2 gap-x-8 gap-y-5">
          <section aria-labelledby={`${headingId}-reasons`} className="min-w-0 flex flex-col gap-2">
            <h3 id={`${headingId}-reasons`} className="text-body font-bold">{t('pstReasons')}</h3>
            {reasons.total === 0 ? <p className="text-[var(--ink-3)]">{t('pstNoFailures')}</p> : <>
              <h4 className="ui-label text-[var(--ink-3)]">{t('pstReasonsMine')}</h4>
              <ul className="flex flex-col gap-2">{reasons.byReason.map(r => <HBar key={r.key ?? ''} value={r.count} max={reasons.byReason[0].count}
                label={r.key === null ? t('pfNone') : reasonText(r.key)} text={share(r.count, reasons.total)} />)}</ul>
              <h4 className="ui-label text-[var(--ink-3)] mt-2">{t('pstReasonsPrinter')}</h4>
              <ul className="flex flex-col gap-2">{reasons.byStatus.map(r => <HBar key={r.key} value={r.count} max={Math.max(...reasons.byStatus.map(x => x.count))}
                label={String(t((GROUP_KEY as Record<string, keyof Translations>)[r.key] ?? 'pstGroupOther'))} text={share(r.count, reasons.total)} />)}</ul>
              <p className="text-caption text-[var(--ink-3)]">{t('pstReasonsNote').replace('{count}', String(reasons.total))}</p>
            </>}
          </section>
          <section aria-labelledby={`${headingId}-material`} className="min-w-0 flex flex-col gap-2">
            <h3 id={`${headingId}-material`} className="text-body font-bold">{t('pstMaterial')}</h3>
            {materials.length === 0 ? <p className="text-[var(--ink-3)]">{t('pstMaterialNone')}</p> : <>
              <ul className="flex flex-col gap-2">{materials.map(m => <HBar key={m.material ?? ''} value={m.grams} max={materials[0].grams}
                label={m.material ?? t('pstMaterialUnknown')} text={gramsText(m.grams)} />)}</ul>
              <p className="text-caption text-[var(--ink-3)]">{t('pstMaterialNote')} {formatCount(t('pstWithoutGrams'), sum.withoutGrams)}</p>
            </>}
          </section>
        </div>

        {effective === ALL && printers.length > 1 && <section aria-labelledby={`${headingId}-per`} className="flex flex-col gap-2">
          <h3 id={`${headingId}-per`} className="text-body font-bold">{t('pstPerPrinter')}</h3>
          <div role="region" aria-label={t('pstPerPrinter')} tabIndex={0} className="overflow-auto">
            <table className="w-full text-left text-small">
              <thead className="ui-label text-[var(--ink-3)]"><tr>
                <th scope="col" className="p-2 border-b border-[var(--line)]">{t('pstPrinter')}</th>
                <th scope="col" className="p-2 border-b border-[var(--line)] text-right">{t('pstColPrints')}</th>
                {OUTCOMES.map(o => <th scope="col" key={o} className="p-2 border-b border-[var(--line)] text-right">{t(OUTCOME_KEY[o])}</th>)}
                <th scope="col" className="p-2 border-b border-[var(--line)] text-right">{t('pstColRate')}</th>
                <th scope="col" className="p-2 border-b border-[var(--line)] text-right">{t('pstColTime')}</th>
                <th scope="col" className="p-2 border-b border-[var(--line)] text-right">{t('pstColFilament')}</th>
              </tr></thead>
              <tbody>{printers.map(p => {
                const own = totals(rows.filter(r => r.printerId === p.id));
                return <tr key={p.id}>
                  <th scope="row" className="p-2 font-normal">{p.name}</th>
                  <td className="p-2 text-right tabular-nums">{own.total}</td>
                  {OUTCOMES.map(o => <td key={o} className="p-2 text-right tabular-nums">{own[o]}</td>)}
                  <td className="p-2 text-right tabular-nums">{own.successRate === null ? '–' : `${own.successRate} %`}</td>
                  <td className="p-2 text-right tabular-nums whitespace-nowrap">{own.total ? duration(own.printSeconds) : '–'}</td>
                  <td className="p-2 text-right tabular-nums whitespace-nowrap">{own.total ? gramsText(own.grams) : '–'}</td>
                </tr>;
              })}</tbody>
            </table>
          </div>
        </section>}
      </>}
    </div>
  </section>;
}
