import { useRef, useState } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import type { ImportJobResult, ImportProgress } from '../types';
import type { ImportJobMeta } from '../hooks/useFileImport';
import { ReportProblemLink } from '../diagnostics/ReportProblemLink';
import { ImportResultDialog } from './ImportResultDialog';
import './import.css';
interface Props {
  progress: ImportProgress;
  meta: ImportJobMeta;
  result: ImportJobResult | null;
  queued: number;
  onCancel: () => void | Promise<void>;
  onDismiss: () => void;
  onSelectModel?: (id: string) => void;
}
export function ImportProgressRow({ progress: p, meta, result, queued, onCancel, onDismiss, onSelectModel }: Props) {
  const t = useT(); const { language } = useLanguage();
  const n = (value: number) => new Intl.NumberFormat(language).format(value);
  const [cancelRequested, setCancelRequested] = useState(false);
  const [details, setDetails] = useState(false);
  const detailsRef = useRef<HTMLButtonElement>(null);
  const state = result?.state ?? p.state;
  const ended = ['finished', 'cancelled', 'failed'].includes(state);
  const cancelling = !ended && (cancelRequested || state === 'cancelling');
  const counts = result?.counts ?? p.counts;
  const source = t(meta.source === 'files' ? 'impFiles' : meta.source === 'dropped' ? 'impDropped' : meta.source === 'archive' ? 'impArchive' : 'impFolder');
  const target = meta.targetName ? t('impTarget').replace('{name}', meta.targetName) : t('impStay');
  const status = t(({ queued: 'impQueued', scanning: 'impScanning', importing: 'impImporting', placing: 'impPlacing', cancelling: 'impCancelling', finished: 'impFinished', cancelled: 'impCancelled', failed: 'impFailed' } as const)[state]);
  const title = state === 'scanning' ? t('impSearching') : state === 'placing' ? t('impMoving').replace('{name}', meta.targetName ?? '') : state === 'finished' ? t('impModelsImported').replace('{n}', n(counts.imported)) : state === 'cancelled' ? t(result?.scanComplete === false ? 'impSearchStopped' : 'impStopped').replace('{n}', n(result?.scanComplete === false ? counts.known : counts.known - (result?.groups.skipped.filter(s => s.reason === 'notStarted').length ?? 0))) : state === 'failed' ? result?.jobError?.message : target;
  const fraction = t(state === 'placing' ? 'impPlaced' : 'impOf').replace('{done}', n(p.done)).replace('{total}', n(p.total ?? counts.known));
  const duration = `${Math.floor(p.elapsedMs / 60000)}:${String(Math.floor(p.elapsedMs / 1000) % 60).padStart(2, '0')}`;
  const message = state === 'scanning' ? `${t('impFound').replace('{n}', n(p.found))}${p.current ? ` · ${p.current}` : ''}` : state === 'finished' ? t('impDuration').replace('{time}', duration) : state === 'cancelled' ? t('impSaved') : state === 'failed' ? t('impFailureHelp').replace('{n}', n(counts.imported + counts.importedNotPlaced)) : `${fraction}${p.current ? ` · ${p.current}` : ''}`;
  return <>
    <section className={`import-row ${state === 'finished' ? 'done' : state === 'cancelled' ? 'cancel' : state === 'failed' ? 'fail' : ''}`} role="status" aria-live="polite">
      <div className="import-title"><span className="import-chip">{status}</span><span>{source} · {title}</span>{queued > 0 && <span className="import-queue">{queued === 1 ? t('impWaitOne') : t('impWaitMany').replace('{n}', n(queued))}</span>}</div>
      <div className="import-actions">{ended ? <><button ref={detailsRef} onClick={() => setDetails(true)}>{t('impDetails')}</button>{state === 'failed' && <ReportProblemLink />}<button onClick={onDismiss}>{t('impClose')}</button></> : <button disabled={cancelling} onClick={() => { setCancelRequested(true); void onCancel(); }}>{cancelling ? t('impCancelling') : t('impCancel')}</button>}</div>
      <div className={`import-bar ${state === 'scanning' || state === 'queued' ? 'indeterminate' : ''}`} role="progressbar" aria-label={status} aria-valuemin={0} aria-valuemax={p.total ?? counts.known} aria-valuenow={state === 'scanning' || state === 'queued' ? undefined : p.done}><i style={state === 'scanning' || state === 'queued' ? undefined : {width: `${Math.min(100, (state === 'finished' ? 1 : p.done / Math.max(1, p.total ?? counts.known)) * 100)}%`}} /></div>
      <div className="import-meta">{state === 'failed' && result?.scanComplete === false && <>{t('impSearchStopped').replace('{n}', n(counts.known))} · </>}{message}</div>
      <div className="import-counts">{([[counts.imported, 'impImported'], [counts.duplicate, 'impDuplicate'], [counts.skipped, 'impSkipped'], [counts.importedNotPlaced, 'impNotPlaced'], [p.inFlight, 'impInFlight']] as const).map(([value, key]) => (value > 0 || key === 'impImported') && <span key={key} className={key === 'impSkipped' ? 'crit' : key === 'impDuplicate' || key === 'impNotPlaced' ? 'warn' : undefined}><b>{n(value)}</b> {t(key)}</span>)}</div>
    </section>
    {details && result && <ImportResultDialog result={result} onClose={() => setDetails(false)} returnFocus={detailsRef} onSelectModel={onSelectModel} />}
  </>;
}
