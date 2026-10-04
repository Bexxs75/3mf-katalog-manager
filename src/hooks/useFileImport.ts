import { useCallback, useEffect, useRef, useState } from 'react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import * as api from '../lib/api/importExport';
import { toAppError } from '../lib/errors';
import type { ArchiveInfo, ArchiveRequest, ImportJobResult, ImportProgress, ImportResultDto, ImportSource, SkippedFile, StartedImport } from '../types';

interface UseFileImportArgs {
  enabled: boolean;
  catalogBaseDir: string | null;
  activeFolderId: string;
  targetName?: string;
  rootFolderId?: string;
  onImported: (result: ImportResultDto) => void;
  refreshFolders: () => void;
  refreshFiles: () => void;
}
export interface ImportJobMeta { jobId: string; source: ImportSource; targetName?: string }
const storageKey = 'catalog-import-jobs';
const terminal = (state: string) => ['finished', 'cancelled', 'failed'].includes(state);
const emptyCounts = { imported: 0, importedNotPlaced: 0, duplicate: 0, skipped: 0, archive: 0, known: 0 };
const initialProgress = (jobId: string): ImportProgress => ({ jobId, state: 'queued', scanComplete: false, total: null, done: 0, inFlight: 0, counts: emptyCounts, current: null, elapsedMs: 0 });

export function useFileImport(args: UseFileImportArgs) {
  const latest = useRef(args); latest.current = args;
  const jobs = useRef(new Map<string, { meta: ImportJobMeta; progress: ImportProgress }>());
  const lastMeta = useRef<ImportJobMeta | null>(null);
  const completed = useRef(new Set<string>());
  const parents = useRef(new Map<string, string>());
  const mounted = useRef(true);
  const [, update] = useState(0);
  const [result, setResult] = useState<ImportJobResult | null>(null);
  const [lastJob, setLastJob] = useState<{ meta: ImportJobMeta; progress: ImportProgress } | null>(null);
  const [error, setError] = useState<ReturnType<typeof toAppError> | null>(null);
  const [pendingArchives, setPendingArchives] = useState<ArchiveInfo[] | null>(null);
  // Setup still uses the legacy DTO and its summary.
  const [importBanner, setImportBanner] = useState<{ imported: number; duplicates: number; skipped: SkippedFile[] } | null>(null);
  const mergeImported = useCallback((value: ImportResultDto) => {
    latest.current.onImported(value);
    if (value.duplicateCount || value.skipped?.length) setImportBanner({ imported: value.imported.length, duplicates: value.duplicateCount, skipped: value.skipped ?? [] });
  }, []);
  const persist = useCallback(() => {
    const remembered = [...jobs.current.values()].map(j => j.meta);
    if (lastMeta.current && !jobs.current.has(lastMeta.current.jobId)) remembered.push(lastMeta.current);
    try { sessionStorage.setItem(storageKey, JSON.stringify(remembered)); } catch { /* Storage may be unavailable. Events still work. */ }
    if (mounted.current) update(v => v + 1);
  }, []);
  const receiveProgress = useCallback((progress: ImportProgress) => {
    if (!mounted.current) return;
    if (completed.current.has(progress.jobId)) return;
    const previous = jobs.current.get(progress.jobId);
    jobs.current.set(progress.jobId, { meta: previous?.meta ?? {jobId: progress.jobId, source: 'files'}, progress });
    persist();
  }, [persist]);
  const receiveFinished = useCallback(async (value: ImportJobResult) => {
    if (!mounted.current || completed.current.has(value.jobId)) return;
    completed.current.add(value.jobId);
    const job = jobs.current.get(value.jobId);
    lastMeta.current = { ...job?.meta, jobId: value.jobId, source: value.source };
    setLastJob({ meta: lastMeta.current, progress: { ...(job?.progress ?? initialProgress(value.jobId)), state: value.state, counts: value.counts, scanComplete: value.scanComplete, inFlight: 0 } });
    jobs.current.delete(value.jobId); setResult(value); persist();
    latest.current.refreshFiles(); latest.current.refreshFolders();
    const paths = value.groups.archive.filter(a => a.state === 'pending').map(a => a.path);
    if (!paths.length) return;
    try {
      const inspected = await api.inspectArchives(paths);
      if (!mounted.current) return;
      for (const path of paths) if (!parents.current.has(path)) parents.current.set(path, value.jobId);
      setPendingArchives(previous => {
        const known = new Set(previous?.map(a => a.path));
        return [...previous ?? [], ...inspected.filter(a => !known.has(a.path))];
      });
    } catch (e) { if (mounted.current) setError(toAppError(e)); }
  }, [persist]);
  const recover = useCallback(async (meta: ImportJobMeta) => {
    const before = jobs.current.get(meta.jobId)?.progress;
    const progress = await api.getImportState(meta.jobId);
    if (!mounted.current || completed.current.has(meta.jobId)) return;
    if (progress && terminal(progress.state)) {
      const value = await api.getImportResult(meta.jobId);
      if (value) await receiveFinished(value);
    } else if (progress && jobs.current.get(meta.jobId)?.progress === before) receiveProgress(progress);
    else if (!progress && jobs.current.get(meta.jobId)?.progress === before) { jobs.current.delete(meta.jobId); persist(); }
  }, [persist, receiveFinished, receiveProgress]);
  const track = useCallback(async (start: Promise<StartedImport>, meta: Omit<ImportJobMeta, 'jobId'>) => {
    setError(null);
    try {
      const {jobId} = await start;
      if (!jobId || !mounted.current) return;
      // Very small jobs can finish before the start command returns its ID.
      if (completed.current.has(jobId)) {
        if (lastMeta.current?.jobId === jobId) lastMeta.current = {...meta, jobId};
        persist();
        setLastJob(previous => previous?.meta.jobId === jobId ? {...previous, meta: {...meta, jobId}} : previous);
        return;
      }
      jobs.current.set(jobId, { meta: {...meta, jobId}, progress: jobs.current.get(jobId)?.progress ?? initialProgress(jobId) });
      persist();
      await recover({...meta, jobId});
    } catch (e) { if (mounted.current) setError(toAppError(e)); }
  }, [persist, recover]);
  useEffect(() => {
    mounted.current = true;
    let disposed = false;
    let saved: ImportJobMeta[] = [];
    try {
      const parsed: unknown = JSON.parse(sessionStorage.getItem(storageKey) ?? '[]');
      if (Array.isArray(parsed)) saved = parsed.filter(m => m && typeof m.jobId === 'string' && ['files','folder','dropped','archive','setupAdopt'].includes(m.source));
    } catch { /* Ignore stale session data. */ }
    for (const meta of saved) {
      if (!jobs.current.has(meta.jobId) && !completed.current.has(meta.jobId)) jobs.current.set(meta.jobId, {meta, progress: initialProgress(meta.jobId)});
    }
    const subscriptions = [api.onImportProgress(receiveProgress), api.onImportFinished(value => { void receiveFinished(value); })];
    // Subscribe before querying so neither registration nor a missed finish leaves a gap.
    void Promise.all(subscriptions).then(async () => {
      if (disposed) return;
      persist();
      await Promise.all(saved.map(recover));
    }).catch(e => { if (!disposed) setError(toAppError(e)); });
    return () => { disposed = true; mounted.current = false; for (const subscription of subscriptions) void subscription.then(off => off()).catch(console.error); };
  }, [persist, receiveProgress, receiveFinished, recover]);
  useEffect(() => {
    const subscription = getCurrentWebview().onDragDropEvent(event => {
      if (event.payload.type === 'drop' && latest.current.enabled) void track(api.startDroppedImport(event.payload.paths), {source: 'dropped'});
    });
    return () => { void subscription.then(off => off()).catch(console.error); };
  }, [track]);
  const active = [...jobs.current.values()].filter(j => !terminal(j.progress.state));
  const current = active.find(j => j.progress.state !== 'queued') ?? active[0] ?? lastJob;
  const cancel = async () => {
    if (!current || terminal(current.progress.state)) return;
    receiveProgress({...current.progress, state: 'cancelling'});
    try { await api.cancelImport(current.meta.jobId); } catch (e) { setError(toAppError(e)); await recover(current.meta); }
  };
  const startArchives = async (target: string, requests: ArchiveRequest[], deleteArchives: boolean) => {
    const grouped = new Map<string, ArchiveRequest[]>();
    for (const request of requests) {
      const parent = parents.current.get(request.path);
      if (!parent) throw new Error('Missing archive parent job');
      grouped.set(parent, [...grouped.get(parent) ?? [], request]);
    }
    for (const [parent, group] of grouped) {
      // Let dialog display submission errors; retain grants for unsubmitted groups.
      const started = await api.startArchiveImport(target, group, deleteArchives, parent);
      for (const request of group) parents.current.delete(request.path);
      setPendingArchives(previous => previous?.filter(a => !group.some(r => r.path === a.path)) ?? null);
      await track(Promise.resolve(started), {source: 'archive', targetName: target});
    }
    const unused = [...parents.current.keys()];
    if (unused.length) await api.discardArchiveImports(unused);
    parents.current.clear(); setPendingArchives(null);
  };
  return {
    jobId: current?.meta.jobId, meta: current?.meta, progress: current?.progress ?? null,
    result, queued: active.filter(j => j.progress.state === 'queued').length,
    jobActive: active.length > 0, error, cancel,
    dismiss: () => { lastMeta.current = null; setResult(null); setLastJob(null); setError(null); persist(); },
    importFiles: () => track(api.startImport('files', args.activeFolderId === 'all' ? args.rootFolderId : args.activeFolderId), {source: 'files', targetName: args.catalogBaseDir ? args.targetName ?? args.catalogBaseDir : undefined}),
    importFolder: () => track(api.startImport('folder'), {source: 'folder'}),
    startArchives, pendingArchives,
    cancelArchives: async () => {
      try { if (pendingArchives?.length) await api.discardArchiveImports(pendingArchives.map(a => a.path)); parents.current.clear(); setPendingArchives(null); }
      catch (e) { setError(toAppError(e)); }
    },
    mergeImported, importBanner, dismissImportBanner: () => setImportBanner(null),
  };
}
