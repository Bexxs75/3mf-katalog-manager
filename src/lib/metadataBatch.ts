import { reportMetadataError } from './metadataErrors';
import { messageOf } from './errors';

/** Four workers, all outcomes collected; no fast-fail while requests are still running. */
export async function metadataBatch<T>(ids: string[], action: (id: string) => Promise<T>, reconcile: () => Promise<unknown>) {
  const failures: string[] = [];
  let cursor = 0;
  await Promise.all(Array.from({ length: Math.min(4, ids.length) }, async () => {
    while (cursor < ids.length) {
      const id = ids[cursor++];
      try { await action(id); }
      catch (error) { failures.push(`${id}: ${messageOf(error)}`); }
    }
  }));
  try { await reconcile(); }
  catch (error) { failures.push(messageOf(error)); }
  if (failures.length) reportMetadataError(failures.join('\n'));
}

