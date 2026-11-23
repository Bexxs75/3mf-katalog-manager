import { invoke } from '../transport';
import type { WebImportJob } from '../webImport';

/** Progress and result of one source copy; it is the same job record the upload import uses. */
export type SourceImportJob = WebImportJob;

/** A finished job never changes again, so polling can stop. */
export const sourceImportFinished = (job: SourceImportJob) => ['completed', 'cancelled', 'failed'].includes(job.state);

/**
 * Starts copying one server folder (root index plus path below it) into the catalog; web only.
 * The source stays untouched. Rejects with the backend error when the job cannot start.
 */
export function startSourceImport(rootIdx: number, rel: string) {
  return invoke<{ jobId: string }>('start_source_import', { rootIdx, rel });
}
// The job shares the upload import's state and cancel commands, so a source copy needs no routes of its own.
export function getSourceImportJob(jobId: string) {
  return invoke<SourceImportJob>('get_import_state', { jobId });
}
export function cancelSourceImport(jobId: string) {
  return invoke<SourceImportJob>('cancel_import', { jobId });
}
