import { useCallback, useEffect, useRef, useState } from 'react';
import { toAppError, type AppError } from '../lib/errors';
import { getSourceCheckOnStart, listSourceFolders, scanSourceFolders } from '../lib/api/sourceFolders';
import type { SourceScan } from '../types';

/** What the notice after a check says; texts are built where they are shown so a language change applies. */
export type SourceNotice =
  | { kind: 'found'; count: number; folderCount: number; folderPath: string; truncated: boolean; skipped: number; unreachable: number }
  | { kind: 'none'; truncated: boolean; skipped: number; unreachable: number }
  | { kind: 'noFolders' }
  | { kind: 'error'; error: AppError };

const fileCount = (scans: SourceScan[]) => scans.reduce((sum, scan) => sum + scan.files.length, 0);

interface Args {
  /** The catalog has finished loading; a start check must not run before. */
  ready: boolean;
  jobActive: boolean;
  /** Called when a manual check finds no source folder, so the user can add one. */
  onNoFolders: () => void;
}

export function useSourceFolderCheck({ ready, jobActive, onNoFolders }: Args) {
  const [scans, setScans] = useState<SourceScan[]>([]);
  const [notice, setNotice] = useState<SourceNotice | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [checking, setChecking] = useState(false);
  const running = useRef(false);
  const mounted = useRef(true);
  const latest = useRef({ jobActive, onNoFolders });
  latest.current = { jobActive, onNoFolders };
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  // `silent` is the check at app start: it only reports new files and never opens anything.
  const check = useCallback(async (silent = false) => {
    if (running.current || latest.current.jobActive) return;
    running.current = true; setChecking(true);
    try {
      const folders = await listSourceFolders();
      if (!mounted.current) return;
      if (folders.length === 0) {
        if (!silent) { setNotice({ kind: 'noFolders' }); latest.current.onNoFolders(); }
        return;
      }
      const result = await scanSourceFolders();
      if (!mounted.current) return;
      const withFiles = result.filter(scan => scan.files.length > 0);
      const extras = {
        truncated: result.some(scan => scan.truncated),
        skipped: result.reduce((sum, scan) => sum + scan.skipped, 0),
        unreachable: result.filter(scan => scan.unreachable).length,
      };
      if (withFiles.length > 0) {
        setScans(result);
        setNotice({ kind: 'found', count: fileCount(result), folderCount: withFiles.length, folderPath: withFiles[0].folderPath, ...extras });
      } else if (!silent) setNotice({ kind: 'none', ...extras });
    } catch (e) {
      if (silent) console.error('Source folder check at start failed', e);
      else if (mounted.current) setNotice({ kind: 'error', error: toAppError(e) });
    } finally {
      running.current = false;
      if (mounted.current) setChecking(false);
    }
  }, []);

  // At most once per session, and only when the user switched it on.
  const startChecked = useRef(false);
  useEffect(() => {
    if (!ready || jobActive || startChecked.current) return;
    startChecked.current = true;
    getSourceCheckOnStart().then(on => on ? check(true) : undefined).catch(e => console.error('Reading the source folder start setting failed', e));
  }, [ready, jobActive, check]);

  /** Re-reads the folders after files were declined; closes the dialog when nothing is left. */
  const rescan = useCallback(async () => {
    const result = await scanSourceFolders();
    if (!mounted.current) return;
    setScans(result);
    if (fileCount(result) === 0) setDialogOpen(false);
  }, []);

  return {
    check, checking, notice, scans, dialogOpen, rescan,
    dismissNotice: useCallback(() => setNotice(null), []),
    openDialog: useCallback(() => { setNotice(null); setDialogOpen(true); }, []),
    closeDialog: useCallback(() => setDialogOpen(false), []),
  };
}
