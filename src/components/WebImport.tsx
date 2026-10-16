import { useEffect, useId, useRef, useState, useSyncExternalStore } from 'react';
import { useModalDialog } from '../hooks/useModalDialog';
import { useT } from '../i18n/LanguageContext';
import { useCanUpload } from '../lib/webSession';
import { importFinished, MODEL_UPLOAD_ACCEPT, WebImportController, type WebImportJob, type UploadTextKey } from '../lib/webImport';
import './import.css';

interface Props { open: boolean; onClose: () => void; onImported: () => void | Promise<unknown> }
const statusKeys: Record<string, UploadTextKey> = {
  queued: 'webUploadQueued', uploading: 'webUploadSending', retrying: 'webUploadRetrying', uploaded: 'webUploadUploaded',
  cancelled: 'webUploadCancelled', importing: 'webUploadImporting', cancelling: 'webUploadCancelling', completed: 'webUploadCompleted',
  imported: 'impImported', imported_not_placed: 'impImported', duplicate: 'impDuplicate', invalid: 'webUploadInvalidFile', error: 'webUploadFailed', failed: 'webUploadFailed',
};
export function WebImport(props: Props) {
  const allowed = useCanUpload();
  return allowed ? <WebImportOwner {...props} /> : null;
}
function WebImportOwner(props: Props) {
  const [controller, setController] = useState<WebImportController | null>(null);
  const latest = useRef(props.onImported); latest.current = props.onImported;
  useEffect(() => {
    const next = new WebImportController();
    next.onImported = () => latest.current();
    setController(next); next.connect();
    return () => next.dispose();
  }, []);
  return controller ? <WebImportSurface {...props} controller={controller} /> : null;
}
function WebImportSurface({ controller, open, onClose }: Props & { controller: WebImportController }) {
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot);
  const t = useT(); const id = useId();
  const [result, setResult] = useState<WebImportJob | null>(null);
  useEffect(() => { if (state.result) setResult(state.result); }, [state.result]);
  const dialog = useModalDialog({ open: open && !result, onClose });
  const uploading = state.queue.some(row => ['queued', 'uploading', 'retrying'].includes(row.state));
  const active = state.jobs.some(job => !importFinished(job));
  const ready = state.uploads.filter(upload => upload.state === 'uploaded').slice(0, 200);
  const label = (state: string) => t(statusKeys[state] ?? 'webUploadFailed');
  return <>
    {!open && (active || uploading || state.connection === 'disconnected' || state.error) && <div className="p-2 border-b border-[var(--line)]" role="status">
      {state.error ? t(state.error) : state.connection === 'disconnected' ? t('webUploadDisconnected') : uploading ? t('webUploadSending') : t('webUploadImporting')}
    </div>}
    {open && !result && <div className="import-backdrop"><div ref={dialog} role="dialog" aria-modal="true" aria-labelledby={`${id}-title`} tabIndex={-1} className="import-result space-y-3">
      <h2 id={`${id}-title`}>{t('webUploadTitle')}</h2>
      <div data-testid="web-upload-drop" className="p-4 border border-dashed border-[var(--line-strong)] rounded"
        onDragOver={event => { event.preventDefault(); event.dataTransfer.dropEffect = 'copy'; }}
        onDrop={event => {
          event.preventDefault();
          const items = Array.from(event.dataTransfer.items ?? []);
          if (items.some(item => item.kind === 'file' && item.webkitGetAsEntry?.()?.isDirectory)) { controller.report({ kind: 'folders' }); return; }
          controller.addFiles(Array.from(event.dataTransfer.files));
        }}>
        <p>{t('webUploadHint')}</p>
        <label className="block mt-3">{t('webUploadChoose')}
          <input className="block max-w-full mt-2" type="file" multiple accept={MODEL_UPLOAD_ACCEPT} onChange={event => {
            controller.addFiles(Array.from(event.currentTarget.files ?? [])); event.currentTarget.value = '';
          }} />
        </label>
      </div>
      {state.connection !== 'connected' && <p role="status">{t(state.connection === 'connecting' ? 'webUploadConnecting' : 'webUploadDisconnected')}</p>}
      {state.error && <p role="alert">{t(state.error)} <button onClick={controller.clearError}>{t('impClose')}</button></p>}
      <div className="max-h-[45vh] overflow-auto space-y-4">
        {state.queue.length > 0 && <ul className="space-y-2">{state.queue.map(row => <li key={row.id}>
          <span className="break-all">{row.name}</span> — <span>{row.error ? t(row.error) : label(row.state)}</span>
          <progress className="block w-full" aria-label={row.name} max={100} value={row.percent} />
          {['queued', 'uploading', 'retrying'].includes(row.state) && <button onClick={() => controller.cancelUpload(row.id)}>{t('webCancel')}</button>}
        </li>)}</ul>}
        <section><h3>{t('webUploadOpen')}</h3><p className="text-small">{t('webUploadDiscardHint')}</p>
          {ready.length > 1 && <button disabled={active || state.busy} onClick={() => void controller.start(ready.map(u => u.uploadId))}>{t('webUploadImportAll')}</button>}
          <ul>{state.uploads.map(upload => <li key={upload.uploadId} className="flex flex-wrap items-center gap-2 py-1">
            <span className="break-all">{upload.name}</span><span>{label(upload.state)}</span>
            {upload.state === 'uploaded' && <button disabled={active || state.busy} onClick={() => void controller.start([upload.uploadId])}>{t('webUploadImport')}</button>}
            <button disabled={state.busy} onClick={() => void controller.discard(upload.uploadId)}>{t('webUploadDiscard')}</button>
          </li>)}</ul>
        </section>
        {state.jobs.length > 0 && <section><h3>{t('webUploadJobs')}</h3><ul>{state.jobs.map(job => <li key={job.jobId} className="py-2">
          <span>{label(job.state)} · {job.done}/{job.total} {job.current}</span>
          {!importFinished(job) ? <><progress className="block w-full" aria-label={t('webUploadImporting')} max={Math.max(1, job.total)} value={job.done} />
            <button disabled={job.state === 'cancelling'} onClick={() => void controller.cancelImport(job.jobId)}>{t('webCancel')}</button></>
            : <button className="ml-2" onClick={() => setResult(job)}>{t('webUploadResults')}</button>}
        </li>)}</ul></section>}
      </div>
      <footer><button onClick={() => void controller.reconcile()}>{t('webRetry')}</button><button onClick={onClose}>{t('impClose')}</button></footer>
    </div></div>}
    {result && <WebImportResult result={result} onClose={() => setResult(null)} />}
  </>;
}
function WebImportResult({ result, onClose }: { result: WebImportJob; onClose: () => void }) {
  const t = useT(); const id = useId();
  const dialog = useModalDialog({ open: true, onClose });
  return <div className="import-backdrop"><div ref={dialog} role="dialog" aria-modal="true" aria-labelledby={`${id}-title`} className="import-result" tabIndex={-1}>
    <h2 id={`${id}-title`}>{t('webImportResultTitle')}</h2>
    <p>{t(statusKeys[result.state] ?? 'webUploadFailed')}</p>
    <div className="import-counts">{(['imported', 'duplicate', 'invalid'] as const).map(state => <span key={state}>
      <b>{result.files.filter(file => file.state === state || (state === 'imported' && file.state === 'imported_not_placed')).length}</b> {t(statusKeys[state])}
    </span>)}</div>
    {(result.state !== 'completed' || result.files.some(file => file.state === 'uploaded')) && <p>{t('webUploadPartial')}</p>}
    {result.kind && <p role="alert">{t(result.kind === 'restart_required' || result.kind === 'journal' ? 'webUploadRestart' : 'webUploadFailed')}</p>}
    <div className="import-table-scroll"><table><thead><tr><th>{t('impFile')}</th><th>{t('impReason')}</th></tr></thead><tbody>
      {result.files.map(file => <tr key={file.uploadId}><td className="break-all">{file.name}</td><td>{t(statusKeys[file.state] ?? 'webUploadFailed')}
        {file.existingFileId && <> · {t('impExistingModel').replace('{id}', file.existingFileId)}</>}
      </td></tr>)}
    </tbody></table></div><footer><button onClick={onClose}>{t('impClose')}</button></footer>
  </div></div>;
}
