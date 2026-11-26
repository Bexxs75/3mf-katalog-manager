import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '../transport';
import { cancelSourceImport, getSourceImportJob, sourceImportFinished, startSourceImport, type SourceImportJob } from './sourceImport';

vi.mock('../transport', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); vi.mocked(invoke).mockResolvedValue({ jobId: 'j1' }); });

const job = (state: string): SourceImportJob => ({ jobId: 'j1', state, current: null, total: 0, done: 0, files: [], kind: null });

describe('source import api', () => {
  it('starts the copy with the root index and the relative path only', async () => {
    await expect(startSourceImport(1, 'Tech/Halter')).resolves.toEqual({ jobId: 'j1' });
    expect(invoke).toHaveBeenCalledWith('start_source_import', { rootIdx: 1, rel: 'Tech/Halter' });
  });
  it('reads and cancels the job through the shared import commands', async () => {
    await getSourceImportJob('j1');
    await cancelSourceImport('j1');
    expect(invoke).toHaveBeenCalledWith('get_import_state', { jobId: 'j1' });
    expect(invoke).toHaveBeenCalledWith('cancel_import', { jobId: 'j1' });
  });
  it('treats completed, cancelled and failed as finished', () => {
    expect(['queued', 'importing', 'cancelling'].some(s => sourceImportFinished(job(s)))).toBe(false);
    expect(['completed', 'cancelled', 'failed'].every(s => sourceImportFinished(job(s)))).toBe(true);
  });
});
