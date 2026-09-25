import { Fragment, useEffect, useMemo, useRef, useState } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatCount } from '../i18n/types';
import { formatDateTime, formatDurationMinutes, formatLengthMm, formatStockG } from '../i18n/format';
import { messageOf } from '../lib/errors';
import { getPrinterJobThumbnail } from '../lib/api/printerLink';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import type { FilamentSpool, PrinterJob } from '../types';
import { ModelPicker, type ModelOption } from './ModelPicker';
import { SpoolPicker } from './SpoolPicker';

interface Props {
  open: boolean;
  jobs: PrinterJob[];
  spools: FilamentSpool[];
  models: ModelOption[];
  link: PrinterLinkState;
  onClose: () => void;
  /** Nach erfolgreichem Buchen (Spulen/Katalog neu laden). */
  onBooked: () => void;
}

interface RowState {
  spoolId: string | null;
  fileId: string | null;
  grams: number | null;
  mismatch: boolean;
  /** Wurde die Spule ausdruecklich vom Nutzer gewaehlt (statt vom
   * Vorschlag uebernommen)? Verhindert, dass eine spaeter eintreffende
   * `spools`-Liste die Wahl mit dem Vorschlag ueberschreibt, erlaubt aber
   * umgekehrt, den Vorschlag noch anzuwenden, sobald die vorgeschlagene
   * Spule verfuegbar wird (z.B. wenn `spools` erst nach `jobs` laedt). */
  userChosenSpool: boolean;
}

function Thumb({ job }: { job: PrinterJob }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    if (job.hasThumbnail) getPrinterJobThumbnail(job.id).then((b) => alive && b && setSrc(`data:image/png;base64,${b}`)).catch(() => undefined);
    return () => { alive = false; };
  }, [job.id, job.hasThumbnail]);
  return src ? (
    <img src={src} alt="" className="w-14 h-14 rounded-md object-cover border border-[var(--line)]" />
  ) : (
    <div aria-hidden className="w-14 h-14 rounded-md border border-[var(--line)] bg-[var(--panel-2)]" />
  );
}

const stripExt = (name: string) => name.replace(/\.(b?gcode)$/i, '');

export function PrinterJobsDialog({ open, jobs, spools: allSpools, models, link, onClose, onBooked }: Props) {
  const t = useT();
  // Nur Filament kann abgebucht werden: Resin-Flaschen sind weder Vorschlag
  // noch Auswahl (das Backend lehnt sie beim Bestaetigen ohnehin ab).
  const spools = useMemo(() => allSpools.filter((s) => s.kind !== 'resin'), [allSpools]);
  const { language } = useLanguage();
  const [rows, setRows] = useState<Record<string, RowState>>({});
  const [picking, setPicking] = useState<string | null>(null);
  // DOM-Knoten des gerade angeklickten Modell-Auswahl-Knopfs - wird im
  // onClick unten manuell gesetzt (nicht per JSX-ref), da derselbe Knopf pro
  // Auftrag existiert und ModelPicker ihre Popup-Position daran ausrichtet.
  const modelAnchorRef = useRef<HTMLElement | null>(null);
  const [failed, setFailed] = useState(0);
  const [actionError, setActionError] = useState<string | null>(null);
  // Auftrags-IDs mit einer laufenden Bestaetigen/Ignorieren-Anfrage - blockt
  // Doppelklicks (und "Alle bestaetigen" waehrend eine Einzelzeile laeuft)
  // davor, den Backend-Aufruf doppelt auszuloesen.
  const [pending, setPending] = useState<Set<string>>(new Set());

  useEffect(() => {
    setRows((prev) => {
      const next: Record<string, RowState> = {};
      for (const j of jobs) {
        const existing = prev[j.id];
        if (existing) {
          let spoolId = existing.spoolId;
          if (spoolId !== null && !spools.some((s) => s.id === spoolId)) {
            // Die gewaehlte Spule (Vorschlag oder Nutzerwahl) ist inzwischen
            // verschwunden (geloescht, oder eine Sicherung mit weniger
            // Spulen wiederhergestellt) - dann muss die Auswahl geleert
            // werden, sonst laesst sich mit einer nicht mehr existierenden
            // Spule "bestaetigen".
            spoolId = null;
          } else if (
            !existing.userChosenSpool &&
            spoolId === null &&
            j.suggestedSpoolId !== null &&
            spools.some((s) => s.id === j.suggestedSpoolId)
          ) {
            // Noch keine Nutzerwahl getroffen, und die vorgeschlagene Spule
            // ist jetzt in `spools` vorhanden (z.B. weil sie erst nach den
            // Auftraegen nachgeladen wurde) - Vorschlag jetzt anwenden.
            spoolId = j.suggestedSpoolId;
          }
          next[j.id] = spoolId === existing.spoolId ? existing : { ...existing, spoolId };
        } else {
          const suggested = j.suggestedSpoolId !== null && spools.some((s) => s.id === j.suggestedSpoolId) ? j.suggestedSpoolId : null;
          next[j.id] = {
            spoolId: suggested,
            fileId: j.modelMatch?.fileId ?? null,
            grams: j.grams,
            mismatch: j.materialMismatch,
            userChosenSpool: false,
          };
        }
      }
      return next;
    });
  }, [jobs, spools]);

  // Direkter Fokus auf den Dialog, damit Escape sofort wirkt (ohne
  // vorherigen Klick ins Fenster).
  useEffect(() => {
    if (open) document.querySelector<HTMLElement>('[aria-labelledby="printer-jobs-title"]')?.focus();
  }, [open]);

  const spoolById = useMemo(() => new Map(spools.map((s) => [s.id, s])), [spools]);
  const modelById = useMemo(() => new Map(models.map((m) => [m.id, m])), [models]);

  if (!open) return null;

  const withPending = async (ids: string[], run: () => Promise<void>) => {
    setPending((p) => new Set([...p, ...ids]));
    try {
      await run();
    } finally {
      setPending((p) => {
        const next = new Set(p);
        ids.forEach((id) => next.delete(id));
        return next;
      });
    }
  };

  const setSpool = (job: PrinterJob, spoolId: string) => {
    setActionError(null);
    setRows((r) => ({ ...r, [job.id]: { ...r[job.id], spoolId, userChosenSpool: true } }));
    link
      .previewJob(job.id, spoolId)
      .then((p) => {
        // Waehrenddessen wurde vielleicht schon wieder eine andere Spule
        // gewaehlt (bzw. eine schnellere Folgeanfrage kam frueher zurueck) -
        // eine veraltete Antwort darf den aktuellen Stand nicht ueberschreiben.
        setRows((r) => {
          const current = r[job.id];
          if (!current || current.spoolId !== spoolId) return r;
          return { ...r, [job.id]: { ...current, grams: p.grams, mismatch: p.materialMismatch } };
        });
      })
      .catch((e) => setActionError(messageOf(e)));
  };

  const confirm = (ids: string[]) => {
    const decisions = ids
      .map((id) => ({ id, row: rows[id] }))
      .filter(({ row }) => row?.spoolId)
      .map(({ id, row }) => ({ jobId: id, spoolId: row.spoolId as string, fileId: row.fileId }));
    if (decisions.length === 0) return;
    const confirmIds = decisions.map((d) => d.jobId);
    return withPending(confirmIds, async () => {
      setActionError(null);
      setFailed(0);
      try {
        const r = await link.confirmJobs(decisions);
        setFailed(r.failed);
        if (r.confirmed > 0) onBooked();
      } catch (e) {
        setFailed(0);
        setActionError(messageOf(e));
      }
    });
  };

  const ignore = (jobId: string) =>
    withPending([jobId], async () => {
      setActionError(null);
      try {
        await link.ignoreJob(jobId);
      } catch (e) {
        setActionError(messageOf(e));
      }
    });

  const bookable = jobs.filter((j) => rows[j.id]?.spoolId);
  const total = bookable.reduce((s, j) => s + (rows[j.id]?.grams ?? 0), 0);
  const restBySpool = new Map<string, number>();
  for (const j of bookable) {
    const r = rows[j.id];
    const sid = r.spoolId as string;
    const start = restBySpool.get(sid) ?? spoolById.get(sid)?.remainingWeightG ?? 0;
    restBySpool.set(sid, Math.max(0, start - (r.grams ?? 0)));
  }
  const restText = [...restBySpool.values()].map((g) => formatStockG(Math.round(g * 10) / 10, language)).join(', ');
  const allConfirmDisabled = bookable.length === 0 || bookable.some((j) => pending.has(j.id));
  const multiplePrinters = new Set(jobs.map((j) => j.printerId)).size > 1;

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/50 p-4" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="printer-jobs-title"
        tabIndex={-1}
        onKeyDown={(e) => e.key === 'Escape' && onClose()}
        className="w-full max-w-[1040px] max-h-[90vh] flex flex-col rounded-[10px] border border-[var(--line-strong)] bg-[var(--panel)] shadow-[var(--shadow)] outline-0"
      >
        <div className="flex items-start justify-between gap-4 px-[18px] py-4 border-b border-[var(--line)]">
          <div>
            <h2 id="printer-jobs-title" className="text-[17px] font-semibold">{t('printerJobsDialogTitle')}</h2>
            <p className="text-[13px] text-[var(--ink-2)]">{t('printerJobsDialogHint')}</p>
          </div>
          <button type="button" aria-label={t('printersClose')} onClick={onClose} className="text-[var(--ink-2)] cursor-pointer">✕</button>
        </div>

        <div className="flex-1 overflow-y-auto">
          {jobs.map((job, i) => {
            const row = rows[job.id] ?? { spoolId: null, fileId: null, grams: null, mismatch: false, userChosenSpool: false };
            const spool = row.spoolId ? spoolById.get(row.spoolId) : undefined;
            const model = row.fileId ? modelById.get(row.fileId) ?? (job.modelMatch?.fileId === row.fileId ? { id: row.fileId, name: job.modelMatch.fileName } : undefined) : undefined;
            const isSuggestedModel = !!model && job.modelMatch?.fileId === model.id;
            const showHeader = i === 0 || jobs[i - 1].printerId !== job.printerId;
            const busy = pending.has(job.id);
            const chip =
              job.outcome === 'completed'
                ? <span className="font-mono-ui text-[11px] font-semibold px-1.5 py-0.5 rounded bg-[var(--good-soft)] text-[var(--good)]">{t('printerJobCompleted')}</span>
                : <span className="font-mono-ui text-[11px] font-semibold px-1.5 py-0.5 rounded bg-[var(--warn-soft)] text-[var(--warn)]">
                    {job.partialPercent !== null ? t('printerJobPartialPercent').replace('{percent}', () => String(job.partialPercent)) : t('printerJobPartial')}
                  </span>;
            return (
              <Fragment key={job.id}>
                {showHeader && multiplePrinters && (
                  <div className="px-[18px] pt-3 font-mono-ui text-[11px] uppercase tracking-[0.12em] text-[var(--ink-3)]">{job.printerName}</div>
                )}
                <div className="grid grid-cols-[56px_minmax(160px,1.3fr)_100px_minmax(160px,1fr)_minmax(180px,1.1fr)_120px] gap-3.5 px-[18px] py-3.5 border-b border-[var(--line)] items-start">
                  <Thumb job={job} />
                  <div className="min-w-0">
                    <div className="font-semibold text-[13.5px] break-words">{stripExt(job.fileName)}</div>
                    <div className="font-mono-ui text-[11.5px] text-[var(--ink-3)] mt-0.5">
                      {formatDateTime(job.endedAt, language)} · {t('printerJobsMinutes').replace('{min}', () => formatDurationMinutes(job.printDurationS, language))}
                    </div>
                    <div className="mt-1.5">{chip}</div>
                  </div>
                  <div className="font-mono-ui text-[15px] font-semibold">
                    {row.grams !== null ? formatStockG(row.grams, language) : '–'}
                    <small className="block text-[11px] font-normal text-[var(--ink-3)]">{formatLengthMm(job.usedMm, language)}</small>
                  </div>
                  <div className="flex flex-col gap-1">
                    <SpoolPicker spools={spools} value={row.spoolId} onChange={(id) => setSpool(job, id)} label={t('printerJobsColSpool')} placeholder={t('printerJobChooseSpool')} />
                    {row.mismatch && spool && job.material ? (
                      <span className="text-[12px] text-[var(--warn)]">⚠ {t('printerJobMaterialWarning').replace('{job}', () => job.material as string).replace('{spool}', () => spool.material)}</span>
                    ) : row.spoolId && row.spoolId === job.suggestedSpoolId ? (
                      <span className="text-[11.5px] text-[var(--ink-3)]">{t('printerJobLoadedIn').replace('{printer}', () => job.printerName)}</span>
                    ) : null}
                  </div>
                  <div className="relative flex flex-col gap-1">
                    <button
                      type="button"
                      onClick={(e) => {
                        modelAnchorRef.current = e.currentTarget;
                        setPicking(picking === job.id ? null : job.id);
                      }}
                      className="flex items-center gap-2 rounded-md border border-dashed border-[var(--line-strong)] px-2 py-1.5 text-[12.5px] text-left cursor-pointer"
                    >
                      {model ? (
                        <>
                          <span className={isSuggestedModel && job.modelMatch?.sure ? 'text-[var(--good)]' : 'text-[var(--ink-3)]'}>
                            {isSuggestedModel && job.modelMatch?.sure ? '✓' : '?'}
                          </span>
                          <span className="truncate">{model.name}</span>
                          <span className="ml-auto font-mono-ui text-[11px] text-[var(--ink-3)]">{t('printerJobModelChange')}</span>
                        </>
                      ) : (
                        <>
                          <span className="text-[var(--ink-3)]">{t('printerJobModelNone')}</span>
                          <span className="ml-auto font-mono-ui text-[11px] text-[var(--ink-3)]">{t('printerJobModelChoose')}</span>
                        </>
                      )}
                    </button>
                    <span className="text-[11.5px] text-[var(--ink-3)]">
                      {model ? (isSuggestedModel && !job.modelMatch?.sure ? t('printerJobModelUnsureHint') : t('printerJobModelSureHint')) : t('printerJobModelNoneHint')}
                    </span>
                    {picking === job.id && (
                      <ModelPicker
                        models={models}
                        anchorRef={modelAnchorRef}
                        onClose={() => setPicking(null)}
                        onChange={(fileId) => {
                          setRows((r) => ({ ...r, [job.id]: { ...r[job.id], fileId } }));
                          setPicking(null);
                        }}
                      />
                    )}
                  </div>
                  <div className="flex flex-col gap-1.5">
                    <button
                      type="button"
                      disabled={!row.spoolId || busy}
                      onClick={() => confirm([job.id])}
                      className="h-7 px-2 rounded-md border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12px] font-semibold cursor-pointer disabled:opacity-50"
                    >
                      {t('printerJobConfirm')}
                    </button>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => ignore(job.id)}
                      className="h-7 px-2 text-[12px] text-[var(--ink-2)] cursor-pointer disabled:opacity-50"
                    >
                      {t('printerJobIgnore')}
                    </button>
                  </div>
                </div>
              </Fragment>
            );
          })}
        </div>

        <div className="flex flex-wrap items-center justify-between gap-3 px-[18px] py-3 bg-[var(--panel-2)] rounded-b-[10px]">
          <span className="text-[13px] text-[var(--ink-2)]">
            {t('printerJobsTotal')
              .replace('{grams}', () => formatStockG(Math.round(total * 10) / 10, language))
              .replace('{rest}', () => restText || '–')}
            {failed > 0 && <span className="block text-[var(--crit)]">{formatCount(t('printerJobsConfirmFailed'), failed)}</span>}
            {actionError && <span role="alert" className="block text-[var(--crit)]">{t('printerConnectionActionFailed').replace('{message}', () => actionError)}</span>}
          </span>
          <div className="flex gap-2.5">
            <button type="button" onClick={onClose} className="h-8 px-3 rounded-md border border-[var(--line-strong)] text-[12.5px] cursor-pointer">
              {t('printerJobsLater')}
            </button>
            <button
              type="button"
              disabled={allConfirmDisabled}
              onClick={() => confirm(bookable.map((j) => j.id))}
              className="h-8 px-3 rounded-md border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-bold cursor-pointer disabled:opacity-50"
            >
              {t('printerJobsConfirmAll').replace('{count}', () => String(bookable.length))}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
