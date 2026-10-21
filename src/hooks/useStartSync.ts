import { useEffect, useRef, useState } from 'react';
import { runStartSync, type StartSyncResult } from '../lib/api/diskSync';

/** What the start notice has to say; counts are already limited to what matters. */
export interface StartSyncNotice {
  /** Set with automatic take-over: what was added, even if it was nothing but failures. */
  added: { files: number; renames: number; failed: number } | null;
  newFiles: number; renamed: number; missing: number; duplicates: number; unreachable: number;
}

/** Turns the backend result into a notice, or null when there is nothing to tell. */
export function startSyncNotice(result: StartSyncResult): StartSyncNotice | null {
  if (!result.ran) return null;
  const base = { newFiles: 0, renamed: 0, missing: 0, duplicates: 0, unreachable: result.unreachableRoots };
  // Unreachable folders suppress everything else on the backend; never read them as "missing".
  if (result.unreachableRoots > 0) return { ...base, added: null };
  const applied = result.applied;
  const notice: StartSyncNotice = applied
    ? { ...base, added: { files: applied.added, renames: applied.renamed, failed: applied.failed }, missing: result.missingFiles, duplicates: result.duplicates }
    : { ...base, added: null, newFiles: result.newFiles, renamed: result.renamed, missing: result.missingFiles, duplicates: result.duplicates };
  const added = notice.added;
  const anything = (added ? added.files + added.renames + added.failed : 0) + notice.newFiles + notice.renamed + notice.missing + notice.duplicates;
  return anything > 0 ? notice : null;
}

// Once per app session, not per component instance: remounting must not scan again.
let startSyncStarted = false;
/** Test hook: starts a new "session". */
export function resetStartSyncSession() { startSyncStarted = false; }

/**
 * Runs the sync with the disk once when the catalog is ready. The call runs in the
 * background; `onApplied` reloads lists when files were taken over automatically.
 */
export function useStartSync(ready: boolean, onApplied: () => Promise<unknown>) {
  const [notice, setNotice] = useState<StartSyncNotice | null>(null);
  const appliedRef = useRef(onApplied);
  appliedRef.current = onApplied;
  useEffect(() => {
    if (!ready || startSyncStarted) return;
    startSyncStarted = true;
    void (async () => {
      try {
        const result = await runStartSync();
        if (result.applied && (result.applied.added > 0 || result.applied.renamed > 0)) {
          try { await appliedRef.current(); } catch (e) { console.error('Catalog refresh after start sync failed', e); }
        }
        setNotice(startSyncNotice(result));
      } catch (e) {
        // A failed background check is not worth interrupting anyone; F5 shows real errors.
        console.error('Start sync failed', e);
      }
    })();
  }, [ready]);
  return { notice, dismiss: () => setNotice(null) };
}
