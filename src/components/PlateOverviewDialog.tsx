import { useEffect, useId, useMemo, useState, type ReactNode } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatLayerHeightMm, formatNozzleMm, formatPrintDuration, formatTemperatureC, formatUsageG } from '../i18n/format';
import { displayName } from '../lib/displayName';
import { readPlateOverview } from '../lib/api/files';
import { sumPrintTime, sumWeightG, summarizePrintData, type PlateRow, type SharedValue } from '../lib/printData';
import { useCanEditMetadata } from '../lib/webSession';
import { useModalDialog } from '../hooks/useModalDialog';
import type { ModelFile, PlateOverviewRow } from '../types';
import { Icon } from './Icon';

type Source = Pick<ModelFile, 'id' | 'name' | 'path' | 'sliceInfo' | 'printInfo' | 'summaryOnly'>;

interface Props {
  model: Source;
  onClose: () => void;
  /** Reads the print data of an unread file; omitted where the user may not change the catalog. */
  onReread?: (id: string) => void;
}

type Overview = { status: 'loading' } | { status: 'ready'; rows: Map<number, PlateOverviewRow> } | { status: 'unavailable' };

interface Row {
  plateIndex: number;
  name: string | null;
  printSeconds: number | null;
  weightG: number | null;
  objectCount: number | null;
  preview: string | null;
}

const focusClass = 'focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]';
const buttonClass = `h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[length:var(--font-size-control)] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] ${focusClass}`;

/**
 * Name, objects, time, weight and (on request) the picture of each plate of a 3MF. Name, time and weight
 * come from the catalog; objects and pictures are read from the file when the dialog opens. A value the
 * file does not give is a dash, never a 0, and a sum over incomplete plates says "at least".
 */
export function PlateOverviewDialog({ model, onClose, onReread }: Props) {
  const t = useT();
  const { language } = useLanguage();
  const canEdit = useCanEditMetadata();
  const titleId = useId();
  const viewName = useId();
  const dialogRef = useModalDialog({ open: true, onClose, initialFocus: '[data-initial-focus]' });
  const [view, setView] = useState<'table' | 'tiles'>('table');
  const [showPreview, setShowPreview] = useState(true);
  const [overview, setOverview] = useState<Overview>({ status: 'loading' });

  useEffect(() => {
    let current = true;
    setOverview({ status: 'loading' });
    readPlateOverview(model.id, true).then(
      answer => { if (current) setOverview({ status: 'ready', rows: new Map(answer.plates.map(row => [row.plateIndex, row])) }); },
      () => { if (current) setOverview({ status: 'unavailable' }); },
    );
    return () => { current = false; };
  }, [model.id]);

  // The catalog values only exist once the full record is loaded; until then the dialog waits.
  const loaded = !model.summaryOnly;
  const summary = useMemo(() => summarizePrintData(model), [model]);
  const rows: Row[] = useMemo(() => {
    if (!loaded) return [];
    const byIndex = new Map<number, Row>();
    for (const plate of summary.plates) {
      byIndex.set(plate.plateIndex, { plateIndex: plate.plateIndex, name: plate.name, printSeconds: plate.printSeconds, weightG: plate.weightG, objectCount: null, preview: null });
    }
    // A plate with print time but no filament usage is only in the print info.
    if (model.printInfo?.readerCurrent && model.printInfo.sliceState === 'sliced') {
      for (const info of model.printInfo.plates) {
        if (byIndex.has(info.plateIndex)) continue;
        byIndex.set(info.plateIndex, { plateIndex: info.plateIndex, name: info.name, printSeconds: info.printSeconds, weightG: null, objectCount: null, preview: null });
      }
    }
    if (overview.status === 'ready') {
      for (const [plateIndex, found] of overview.rows) {
        const row = byIndex.get(plateIndex) ?? { plateIndex, name: null, printSeconds: null, weightG: null, objectCount: null, preview: null };
        byIndex.set(plateIndex, { ...row, objectCount: found.objectCount ?? null, preview: found.preview ?? null });
      }
    }
    return [...byIndex.values()].sort((a, b) => a.plateIndex - b.plateIndex);
  }, [loaded, summary, model.printInfo, overview]);

  const unread = loaded && summary.state === 'unread';
  const number = (value: number) => value.toLocaleString(language);
  const duration = (seconds: number) => formatPrintDuration(seconds, language, t);
  const atLeast = (value: string, complete: boolean) => complete ? value : t('pfAtLeast').replace('{value}', value);
  const missing = <><span aria-hidden="true" className="text-[var(--ink-2)]">–</span><span className="sr-only">{t('poNotGiven')}</span></>;
  const waiting = <span aria-hidden="true" className="text-[var(--ink-3)]">…</span>;

  const plateRows: PlateRow[] = rows.map(row => ({ plateIndex: row.plateIndex, name: row.name, printSeconds: row.printSeconds, weightG: row.weightG, weightUnknown: false, filaments: [] }));
  const time = sumPrintTime(plateRows);
  const weightKnown = rows.filter(row => row.weightG !== null).length;
  const weight = sumWeightG(plateRows);
  const objectsKnown = rows.filter(row => row.objectCount !== null).length;
  const objects = objectsKnown === 0 ? null : rows.reduce((sum, row) => sum + (row.objectCount ?? 0), 0);
  const objectsFinal = overview.status !== 'loading';
  const partial = time.known < rows.length || weightKnown < rows.length || (objectsFinal && objectsKnown < rows.length);
  const hasPreview = showPreview && overview.status === 'ready';

  const cell = {
    objects: (row: Row) => row.objectCount !== null ? number(row.objectCount) : overview.status === 'loading' ? waiting : missing,
    time: (row: Row) => row.printSeconds !== null ? duration(row.printSeconds) : missing,
    weight: (row: Row) => row.weightG !== null ? formatUsageG(row.weightG, language) : missing,
    name: (row: Row) => row.name !== null && row.name !== '' ? displayName(row.name) : missing,
  };
  const totals = {
    objects: objects === null ? (overview.status === 'loading' ? waiting : missing) : atLeast(number(objects), !objectsFinal || objectsKnown === rows.length),
    time: time.seconds === null ? missing : atLeast(duration(time.seconds), time.known === rows.length),
    weight: weight === null ? missing : atLeast(formatUsageG(weight, language), weightKnown === rows.length),
  };

  const shared = (value: SharedValue<string | number>, format: (value: string | number) => string): ReactNode =>
    value.kind === 'value' ? format(value.value) : value.kind === 'mixed' ? t('printDataMixed') : missing;
  const sharedEntries: { label: string; value: ReactNode }[] = summary.sharedKind === 'none' ? [] : [
    { label: t('printDataPrinter'), value: shared(summary.shared.printerModel, String) },
    { label: t('printDataNozzle'), value: shared(summary.shared.nozzleMm, v => formatNozzleMm(Number(v), language)) },
    { label: t('printDataLayerHeight'), value: shared(summary.shared.layerHeightMm, v => formatLayerHeightMm(Number(v), language)) },
    { label: t('printDataBedType'), value: shared(summary.shared.bedType, String) },
    ...(summary.sharedKind === 'full' ? [
      { label: t('printDataNozzleTemp'), value: shared(summary.shared.nozzleTempC, v => formatTemperatureC(Number(v), language)) },
      { label: t('printDataBedTemp'), value: shared(summary.shared.bedTempC, v => formatTemperatureC(Number(v), language)) },
    ] : []),
  ];

  const th = 'ui-label text-[var(--ink-3)] font-semibold py-2 pr-4 text-left';
  const thRight = 'ui-label text-[var(--ink-3)] font-semibold py-2 pl-4 text-right';
  const tdRight = 'py-2 pl-4 text-right tabular-nums';
  const preview = (row: Row, className: string) => row.preview
    ? <img src={row.preview} alt="" className={`${className} object-contain bg-[var(--panel-2)]`} />
    : <div aria-hidden="true" className={`${className} bg-[var(--panel-2)] border border-dashed border-[var(--line)]`} />;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-3">
      <div ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby={titleId} tabIndex={-1}
        className="w-[720px] max-w-full max-h-full overflow-y-auto bg-[var(--panel)] text-[var(--ink)] border border-[var(--line-strong)] rounded-[10px] shadow-[var(--shadow)] px-4 pt-3.5 pb-4">
        <div className="flex items-start gap-2 mb-2">
          <div className="min-w-0 flex-1">
            <h2 id={titleId} className="m-0 text-[length:var(--font-size-title)] font-bold break-words">{t('poTitle')}: {displayName(model.name)}</h2>
            <div className="font-code text-caption text-[var(--ink-3)] break-all">{displayName(model.path)}</div>
          </div>
          <button type="button" data-initial-focus onClick={onClose} aria-label={t('poClose')}
            className={`flex-none w-[30px] h-[30px] grid place-items-center rounded-md text-[var(--ink-2)] hover:bg-[var(--panel-2)] cursor-pointer ${focusClass}`}>
            <Icon name="close" size={16} />
          </button>
        </div>

        {!loaded ? <p role="status" className="m-0 py-3 text-small text-[var(--ink-2)]">{t('poLoading')}</p> : (
          <>
            {unread && (
              <div className="rounded-md border border-dashed border-[var(--line-strong)] bg-[var(--panel-2)] px-3 py-2.5 my-2 flex flex-wrap items-center gap-3">
                <p className="m-0 flex-1 min-w-[200px] text-small text-[var(--ink-2)]">{t('poUnread')}</p>
                {onReread && canEdit && <button type="button" onClick={() => onReread(model.id)} className={buttonClass}>{t('poReread')}</button>}
              </div>
            )}

            {sharedEntries.length > 0 && (
              <>
                <h3 className="m-0 mt-3 mb-1.5 ui-label text-[var(--ink-3)]">{t('poPrintData')}</h3>
                <dl className="m-0 grid gap-x-6 gap-y-2 [grid-template-columns:repeat(auto-fill,minmax(180px,1fr))]">
                  {sharedEntries.map(entry => (
                    <div key={entry.label} className="min-w-0">
                      <dt className="text-small text-[var(--ink-2)]">{entry.label}</dt>
                      <dd className="m-0 font-medium tabular-nums break-words">{entry.value}</dd>
                    </div>
                  ))}
                </dl>
              </>
            )}

            <h3 className="m-0 mt-4 mb-1.5 ui-label text-[var(--ink-3)]">{t('poTitle')}</h3>
            {overview.status === 'loading' && <p role="status" className="m-0 mb-2 text-small text-[var(--ink-2)]">{t('poLoading')}</p>}
            {overview.status === 'unavailable' && <p role="status" className="m-0 mb-2 text-small text-[var(--ink-2)]">{t('poUnavailable')}</p>}
            {rows.length === 0 ? (
              overview.status === 'loading' ? null : <p className="m-0 py-2 text-small text-[var(--ink-3)]">{t('poNoPlates')}</p>
            ) : (
              <>
                <div className="flex flex-wrap items-center gap-4 mb-2">
                  <fieldset className="m-0 p-0 border-0 inline-flex rounded-md border border-[var(--line-strong)] overflow-hidden">
                    <legend className="sr-only">{t('poViewMode')}</legend>
                    {(['table', 'tiles'] as const).map(kind => (
                      <label key={kind} className={`px-3 py-1 text-small cursor-pointer has-[:checked]:bg-[var(--accent-soft)] has-[:checked]:text-[var(--accent)] has-[:checked]:font-semibold has-[:focus-visible]:outline has-[:focus-visible]:outline-2 has-[:focus-visible]:-outline-offset-2 has-[:focus-visible]:outline-[var(--accent)]`}>
                        <input type="radio" name={viewName} className="sr-only" checked={view === kind} onChange={() => setView(kind)} />
                        {t(kind === 'table' ? 'poViewTable' : 'poViewTiles')}
                      </label>
                    ))}
                  </fieldset>
                  <label className="inline-flex items-center gap-2 text-small cursor-pointer">
                    <input type="checkbox" checked={showPreview} onChange={event => setShowPreview(event.target.checked)} />
                    {t('poShowPreview')}
                  </label>
                </div>

                {view === 'table' ? (
                  <div className="overflow-x-auto">
                    <table className="w-full text-body border-collapse">
                      <caption className="sr-only">{t('poTitle')}</caption>
                      <thead>
                        <tr>
                          {hasPreview && <th scope="col" className={`${th} w-[84px]`}><span className="sr-only">{t('poColPreview')}</span></th>}
                          <th scope="col" className={th}>{t('printDataColPlate')}</th>
                          <th scope="col" className={th}>{t('poColName')}</th>
                          <th scope="col" className={thRight}>{t('poColObjects')}</th>
                          <th scope="col" className={thRight}>{t('printDataTime')}</th>
                          <th scope="col" className={thRight}>{t('printDataWeight')}</th>
                        </tr>
                      </thead>
                      <tbody>
                        {rows.map(row => (
                          <tr key={row.plateIndex} className="border-t border-[var(--line)] align-middle">
                            {hasPreview && <td className="py-2 pr-4">{preview(row, 'w-[72px] h-[54px]')}</td>}
                            <th scope="row" className="py-2 pr-4 text-left font-semibold tabular-nums">{row.plateIndex}</th>
                            <td className="py-2 pr-4 min-w-0 break-words">{cell.name(row)}</td>
                            <td className={tdRight}>{cell.objects(row)}</td>
                            <td className={tdRight}>{cell.time(row)}</td>
                            <td className={tdRight}>{cell.weight(row)}</td>
                          </tr>
                        ))}
                      </tbody>
                      <tfoot>
                        <tr className="border-t border-[var(--line-strong)] font-semibold">
                          {hasPreview && <td />}
                          <th scope="row" colSpan={2} className="py-2 pr-4 text-left">{t('poTotal')}</th>
                          <td className={tdRight}>{totals.objects}</td>
                          <td className={tdRight}>{totals.time}</td>
                          <td className={tdRight}>{totals.weight}</td>
                        </tr>
                      </tfoot>
                    </table>
                  </div>
                ) : (
                  <>
                    <ul className="m-0 p-0 list-none grid gap-2.5 [grid-template-columns:repeat(auto-fill,minmax(180px,1fr))]">
                      {rows.map(row => (
                        <li key={row.plateIndex} className="min-w-0 border border-[var(--line)] rounded-lg overflow-hidden bg-[var(--panel)]">
                          {hasPreview && preview(row, 'w-full aspect-video')}
                          <div className="px-2.5 py-2">
                            <h4 className="m-0 mb-1 text-small font-semibold break-words">{t('viewerPlate').replace('{n}', String(row.plateIndex))}: {cell.name(row)}</h4>
                            <dl className="m-0 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-small">
                              <dt className="text-[var(--ink-2)]">{t('poColObjects')}</dt><dd className="m-0 text-right tabular-nums">{cell.objects(row)}</dd>
                              <dt className="text-[var(--ink-2)]">{t('printDataTime')}</dt><dd className="m-0 text-right tabular-nums">{cell.time(row)}</dd>
                              <dt className="text-[var(--ink-2)]">{t('printDataWeight')}</dt><dd className="m-0 text-right tabular-nums">{cell.weight(row)}</dd>
                            </dl>
                          </div>
                        </li>
                      ))}
                    </ul>
                    <p className="m-0 mt-2 text-small tabular-nums">
                      <strong>{t('poTotal')}:</strong> {totals.objects} · {totals.time} · {totals.weight}
                    </p>
                  </>
                )}
                {partial && <p className="m-0 mt-2 text-small text-[var(--ink-3)]">{t('poPartialNote')}</p>}
              </>
            )}
          </>
        )}
      </div>
    </div>
  );
}
