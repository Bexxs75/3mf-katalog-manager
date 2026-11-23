import { useCallback, useEffect, useRef, useState } from 'react';
import { cancelSourceImport, getSourceImportJob, sourceImportFinished, startSourceImport, type SourceImportJob } from '../lib/api/sourceImport';
import { toAppError, type AppError } from '../lib/errors';

const POLL_MS = 600;
// A single lost answer must not end the progress view while the server keeps copying.
const MAX_POLL_FAILURES = 3;

/**
 * Starts a source copy and follows its job by polling. The job record is the same one the upload
 * import uses, so the plain state command is enough and no event stream has to be shared with it.
 * `onFinished` runs once the copy ended or can no longer be followed, so the catalog is reloaded
 * even when only part of the files arrived.
 */
export function useSourceImport(onFinished?: () => void | Promise<unknown>) {
  const [job, setJob] = useState<SourceImportJob | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const alive = useRef(true);
  const jobId = useRef<string | null>(null);
  const latest = useRef(onFinished);
  latest.current = onFinished;
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);

  const finish = useCallback(async () => {
    try { await latest.current?.(); } catch { /* A failed reload must not hide the result of the copy. */ }
  }, []);

  const follow = useCallback(async (id: string) => {
    let failures = 0;
    while (alive.current) {
      await new Promise(resolve => setTimeout(resolve, POLL_MS));
      if (!alive.current) return;
      try {
        const next = await getSourceImportJob(id);
        failures = 0;
        setJob(next);
        if (sourceImportFinished(next)) { await finish(); return; }
      } catch (e) {
        if (++failures >= MAX_POLL_FAILURES) {
          if (alive.current) setError(toAppError(e));
          await finish();
          return;
        }
      }
    }
  }, [finish]);

  /** Rejects with the backend error when the job does not start; progress problems go to `error`. */
  const start = useCallback(async (rootIdx: number, rel: string) => {
    const { jobId: id } = await startSourceImport(rootIdx, rel);
    if (!alive.current) return;
    jobId.current = id;
    setError(null);
    setJob({ jobId: id, state: 'queued', current: null, total: 0, done: 0, files: [], kind: null });
    void follow(id);
  }, [follow]);

  const cancel = useCallback(async () => {
    if (!jobId.current) return;
    try {
      const next = await cancelSourceImport(jobId.current);
      if (alive.current) setJob(next);
    } catch (e) { if (alive.current) setError(toAppError(e)); }
  }, []);

  /** Forgets a finished job so the dialog can show its normal content again. */
  const reset = useCallback(() => { jobId.current = null; setJob(null); setError(null); }, []);

  const running = job !== null && !sourceImportFinished(job) && error === null;
  return { job, error, running, start, cancel, reset };
}
