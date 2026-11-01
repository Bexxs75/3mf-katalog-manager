import { createContext, createElement, useCallback, useContext, useEffect, useMemo, useRef, useState, type Dispatch, type ReactNode, type SetStateAction } from 'react';
import * as filesApi from '../lib/api/files';
import { toAppError, messageOf, type AppError } from '../lib/errors';
import { retryAfterSeconds } from '../lib/fileOpsErrors';
import { canEditMetadata } from '../lib/webSession';
import { printDataStateOf, rereadTargets, summarizePrintData } from '../lib/printData';
import type { ModelFile } from '../types';
import { invalidateModelImages } from './useModelImages';

/** Where the run was started: the settings block (all sliced files) or the selection bar (chosen files). */
export type RereadOrigin = 'settings' | 'selection';

/**
 * The only fields a read changes in the list. No images (a bulk read would otherwise carry
 * three base64 pictures per file through the list) and no `fullyLoadedIds` entry.
 */
export type RereadPatch = Pick<ModelFile, 'printInfo' | 'printDataState' | 'sliceInfo' | 'estimatedWeightG' | 'weightSource' | 'costEstimate'
  | 'plateCount' | 'materials' | 'fileSizeBytes' | 'contentHash' | 'fileModifiedAt' | 'sourcePlatform' | 'sourceModelId' | 'sourceProfileId' | 'hasThumbnail'>;

export const REREAD_FLUSH_FILES = 50;
export const REREAD_FLUSH_MS = 250;
export const REREAD_IMPORT_WAIT_MS = 2000;
export const REREAD_BUSY_RETRIES = 3;
/** Longest wait named by a Retry-After header that a bulk read honours; a longer one counts as a failure. */
const MAX_RETRY_WAIT_S = 30;

export function patchOf(model: ModelFile): RereadPatch {
  return {
    printInfo: model.printInfo,
    printDataState: printDataStateOf({ path: model.path, printDataState: undefined, printInfo: model.printInfo }),
    sliceInfo: model.sliceInfo,
    estimatedWeightG: model.estimatedWeightG,
    weightSource: model.weightSource,
    costEstimate: model.costEstimate,
    plateCount: model.plateCount,
    materials: model.materials,
    fileSizeBytes: model.fileSizeBytes,
    contentHash: model.contentHash,
    fileModifiedAt: model.fileModifiedAt,
    sourcePlatform: model.sourcePlatform,
    sourceModelId: model.sourceModelId,
    sourceProfileId: model.sourceProfileId,
    hasThumbnail: model.hasThumbnail,
  };
}

/** One pass over the list for a whole batch of patches. */
export function applyRereadPatches(models: ModelFile[], patches: ReadonlyMap<string, RereadPatch>): ModelFile[] {
  return models.map(model => {
    const patch = patches.get(model.id);
    return patch ? { ...model, ...patch } : model;
  });
}

export interface RereadProgress {
  /** Files finished, successfully or not. */
  done: number;
  failed: number;
  /** Read, but single values are missing (`partial`). */
  partial: number;
  paused: boolean;
  firstError: AppError | null;
}

export interface RereadOutcome extends RereadProgress { cancelled: boolean }

export interface RereadDeps {
  rescan: (id: string) => Promise<ModelFile>;
  applyPatches: (patches: ReadonlyMap<string, RereadPatch>) => void;
  invalidate: (id: string) => void;
  wait: (ms: number) => Promise<void>;
  cancelled: () => boolean;
  now: () => number;
  onProgress: (progress: RereadProgress) => void;
}

/** A transient answer of the web server (503/429 with Retry-After): worth repeating. */
function busyWaitMs(error: unknown): number | null {
  const seconds = retryAfterSeconds((error as { retryAfter?: unknown } | null)?.retryAfter);
  return seconds === null || seconds > MAX_RETRY_WAIT_S ? null : Math.max(1, seconds) * 1000;
}

/**
 * Reads the files one after the other (exactly one call at a time) and writes the results into
 * the list in batches: at most every 50 files or 250 ms one `applyPatches`. Pauses while an import
 * runs, repeats a busy web answer up to 3 times, counts every other failure and goes on.
 */
export async function runPrintDataReread(ids: readonly string[], deps: RereadDeps): Promise<RereadOutcome> {
  const progress: RereadProgress = { done: 0, failed: 0, partial: 0, paused: false, firstError: null };
  let patches = new Map<string, RereadPatch>();
  let lastFlush = deps.now();
  const flush = () => {
    if (patches.size === 0) return;
    const batch = patches;
    patches = new Map();
    deps.applyPatches(batch);
    lastFlush = deps.now();
  };
  const report = () => deps.onProgress({ ...progress });

  let cancelled = false;
  for (const id of ids) {
    if (deps.cancelled()) { cancelled = true; break; }
    let busyRetries = 0;
    let interrupted = false;
    for (;;) {
      try {
        const updated = await deps.rescan(id);
        if (progress.paused) { progress.paused = false; report(); }
        patches.set(id, patchOf(updated));
        deps.invalidate(id);
        if (summarizePrintData(updated).state === 'partial') progress.partial += 1;
        break;
      } catch (error) {
        if (messageOf(error) === 'importActive') {
          // A running import owns the catalog: wait, ask again, stay cancellable.
          if (!progress.paused) { progress.paused = true; flush(); report(); }
          await deps.wait(REREAD_IMPORT_WAIT_MS);
          if (deps.cancelled()) { interrupted = true; break; }
          continue;
        }
        const wait = busyWaitMs(error);
        if (wait !== null && busyRetries < REREAD_BUSY_RETRIES) {
          busyRetries += 1;
          await deps.wait(wait);
          if (deps.cancelled()) { interrupted = true; break; }
          continue;
        }
        progress.failed += 1;
        progress.firstError ??= toAppError(error);
        break;
      }
    }
    // Cancelled while waiting: this file was not read, so it does not count as done.
    if (interrupted) { cancelled = true; break; }
    progress.done += 1;
    if (patches.size >= REREAD_FLUSH_FILES || deps.now() - lastFlush >= REREAD_FLUSH_MS) flush();
    report();
  }
  flush();
  progress.paused = false;
  return { ...progress, cancelled };
}

export type RereadStatus = 'idle' | 'running' | 'cancelling' | 'done' | 'cancelled';

export interface RereadState extends RereadProgress {
  status: RereadStatus;
  origin: RereadOrigin | null;
  total: number;
  /** A start was refused because a run is going: the surface that asked says "already running". */
  busyNotice: RereadOrigin | null;
}

export type RereadStartResult = 'started' | 'busy' | 'nothing' | 'readOnly';

export interface PrintDataRereadApi {
  state: RereadState;
  /** `settings` reads every sliced file not read yet, `selection` the given ids. */
  start: (origin: RereadOrigin, selectedIds?: readonly string[]) => RereadStartResult;
  cancel: () => void;
  /** Clears the finished result (and the "already running" notice) that belongs to `origin`. */
  dismiss: (origin: RereadOrigin) => void;
}

const IDLE: RereadState = { status: 'idle', origin: null, total: 0, done: 0, failed: 0, partial: 0, paused: false, firstError: null, busyNotice: null };

const PrintDataRereadContext = createContext<PrintDataRereadApi>({
  state: IDLE, start: () => 'nothing', cancel: () => {}, dismiss: () => {},
});

export const usePrintDataReread = () => useContext(PrintDataRereadContext);

interface ProviderProps {
  models: ModelFile[];
  setModels: Dispatch<SetStateAction<ModelFile[]>>;
  children?: ReactNode;
}

/** Keeps one run for the whole app, so settings and selection bar share the same lock. */
export function PrintDataRereadProvider({ models, setModels, children }: ProviderProps) {
  const [state, setState] = useState<RereadState>(IDLE);
  const modelsRef = useRef(models);
  modelsRef.current = models;
  const runningRef = useRef(false);
  const cancelRef = useRef(false);
  const mountedRef = useRef(true);
  const wakeRef = useRef<(() => void) | null>(null);
  useEffect(() => { mountedRef.current = true; return () => { mountedRef.current = false; cancelRef.current = true; wakeRef.current?.(); }; }, []);

  const start = useCallback<PrintDataRereadApi['start']>((origin, selectedIds = []) => {
    if (!canEditMetadata()) return 'readOnly';
    if (runningRef.current) {
      setState(prev => ({ ...prev, busyNotice: origin }));
      return 'busy';
    }
    const ids = rereadTargets(modelsRef.current, origin === 'settings' ? 'all' : 'selected', new Set(selectedIds));
    if (ids.length === 0) {
      setState({ ...IDLE, status: 'done', origin, busyNotice: null });
      return 'nothing';
    }
    runningRef.current = true;
    cancelRef.current = false;
    setState({ ...IDLE, status: 'running', origin, total: ids.length });
    const deps: RereadDeps = {
      rescan: async id => {
        try {
          return await filesApi.rescanFileMetadata(id, true);
        } catch (error) {
          // The scan itself succeeded when only the stored answer was too large: read the record again.
          const lost = typeof error === 'object' && error !== null
            && ((error as { message?: unknown }).message === 'request_id_result_unavailable' || (error as { kind?: unknown }).kind === 'request_id_result_unavailable');
          if (!lost) throw error;
          const [fresh] = await filesApi.listFilesByIds([id]);
          if (!fresh) throw error;
          return fresh;
        }
      },
      applyPatches: patches => { if (mountedRef.current) setModels(prev => applyRereadPatches(prev, patches)); },
      invalidate: invalidateModelImages,
      wait: ms => new Promise<void>(resolve => {
        const timer = setTimeout(() => { wakeRef.current = null; resolve(); }, ms);
        wakeRef.current = () => { clearTimeout(timer); wakeRef.current = null; resolve(); };
      }),
      cancelled: () => cancelRef.current,
      now: () => Date.now(),
      onProgress: progress => {
        if (mountedRef.current) setState(prev => ({ ...prev, ...progress, status: cancelRef.current ? 'cancelling' : 'running' }));
      },
    };
    void runPrintDataReread(ids, deps).then(outcome => {
      runningRef.current = false;
      if (!mountedRef.current) return;
      setState(prev => ({ ...prev, ...outcome, status: outcome.cancelled ? 'cancelled' : 'done', busyNotice: null }));
    });
    return 'started';
  }, [setModels]);

  const cancel = useCallback(() => {
    if (!runningRef.current) return;
    cancelRef.current = true;
    wakeRef.current?.();
    setState(prev => prev.status === 'running' ? { ...prev, status: 'cancelling' } : prev);
  }, []);

  const dismiss = useCallback((origin: RereadOrigin) => {
    setState(prev => {
      if ((prev.status === 'done' || prev.status === 'cancelled') && prev.origin === origin) return IDLE;
      return prev.busyNotice === origin ? { ...prev, busyNotice: null } : prev;
    });
  }, []);

  const api = useMemo<PrintDataRereadApi>(() => ({ state, start, cancel, dismiss }), [state, start, cancel, dismiss]);
  return createElement(PrintDataRereadContext.Provider, { value: api }, children);
}
