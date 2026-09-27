import { useCallback, useEffect, useRef, useState } from 'react';
import * as api from '../lib/api/updater';
import { getAppVersion, openReleaseUrl } from '../lib/api/update';
import { toAppError, type AppError } from '../lib/errors';

export type UpdatePhase = 'idle' | 'checking' | 'downloading' | 'ready' | 'installing' | 'error';

/** Which action to retry after a failure. The backend only keeps a downloaded update
 * on a failed backup; a failed install consumes it, so `install()` below falls back
 * to a fresh download in that case instead of retrying the install itself. */
type FailedStep = 'download' | 'install';

/** A recheck must not race an update that is already in flight. */
const BUSY_PHASES: UpdatePhase[] = ['downloading', 'ready', 'installing'];

/** Mirrors the backend's `updater::release_page`. Needed because a download can
 * resolve to a version other than the one from the last check (e.g. a newer
 * release went up in between), and the notes link must follow it. */
function releasePage(version: string): string {
  return `https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v${version}`;
}

export interface UpdaterView {
  /** Loaded immediately via `getAppVersion`, independent of `info` (which only
   * arrives once the update check resolves). */
  currentVersion: string;
  info: api.UpdateInfo | null;
  phase: UpdatePhase;
  progress: api.DownloadProgress | null;
  error: AppError | null;
  /** Set when opening the release page fails; kept separate from `error` so a
   * broken link doesn't hide the update-available offer behind a generic
   * "nothing was installed" message. */
  notesError: AppError | null;
  dismissed: boolean;
  checkNow: () => void;
  startUpdate: () => void;
  install: () => void;
  retry: () => void;
  later: () => void;
  dismiss: () => void;
  openNotes: () => void;
}

export function isUpdateCheckDisabled(phase: UpdatePhase): boolean {
  return phase === 'checking' || BUSY_PHASES.includes(phase);
}

export function useUpdater(): UpdaterView {
  const [currentVersion, setCurrentVersion] = useState('');
  const [info, setInfo] = useState<api.UpdateInfo | null>(null);
  const [phase, setPhase] = useState<UpdatePhase>('idle');
  const [progress, setProgress] = useState<api.DownloadProgress | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [notesError, setNotesError] = useState<AppError | null>(null);
  const [dismissed, setDismissed] = useState(false);
  const [failedStep, setFailedStep] = useState<FailedStep | null>(null);

  // Read without adding `phase` to checkNow's dependencies, which would recreate
  // it (and re-fire the mount effect below) on every phase change.
  const phaseRef = useRef(phase);
  phaseRef.current = phase;

  // Plain (non-state) in-flight guards: a state-based check would still let a
  // second call through when both happen before React re-renders (e.g. two
  // clicks in the same tick), since the guard would still read the old phase.
  const downloadBusyRef = useRef(false);
  const installBusyRef = useRef(false);

  const checkNow = useCallback(() => {
    if (BUSY_PHASES.includes(phaseRef.current)) return;
    setDismissed(false);
    setPhase('checking');
    api
      .checkAppUpdate()
      .then(setInfo)
      // The backend itself never rejects for a network problem - it logs that
      // and resolves with availableVersion: null. A rejection here means the
      // IPC call broke down some other way, worth a log line.
      .catch((e) => console.warn('[updater] check failed:', e))
      .finally(() => setPhase((p) => (p === 'checking' ? 'idle' : p)));
  }, []);

  useEffect(() => {
    // Independent of the update check (which may be slow or offline), so the
    // running version is visible right away.
    getAppVersion()
      .then(setCurrentVersion)
      .catch((e) => console.warn('[updater] could not determine the app version:', e));
    checkNow();
  }, [checkNow]);

  const startUpdate = useCallback(() => {
    if (downloadBusyRef.current) return;
    downloadBusyRef.current = true;
    setError(null);
    setFailedStep(null);
    setDismissed(false);
    setProgress({ downloaded: 0, total: null });
    setPhase('downloading');
    api
      .downloadAppUpdate(setProgress)
      .then((version) => {
        downloadBusyRef.current = false;
        // The download can resolve to a different version than the last check
        // showed (e.g. a newer release went up meanwhile) - the "ready" and
        // "installing" texts, and the notes link, must reflect what was
        // actually downloaded and will actually be installed.
        setInfo((prev) => (prev ? { ...prev, availableVersion: version, releaseUrl: releasePage(version) } : prev));
        setPhase('ready');
      })
      .catch((e) => {
        downloadBusyRef.current = false;
        setError(toAppError(e));
        setFailedStep('download');
        setPhase('error');
      });
  }, []);

  const install = useCallback(() => {
    if (installBusyRef.current) return;
    installBusyRef.current = true;
    setError(null);
    setDismissed(false);
    setPhase('installing');
    // On success the app restarts and this promise never settles.
    api.installAppUpdate().catch((e) => {
      installBusyRef.current = false;
      const appError = toAppError(e);
      if (!appError.unexpected) {
        // The backend only rejects like this when there is no pending update
        // left to install (e.g. a previous attempt already consumed and lost
        // it on a failed install, not a failed backup) - retrying the install
        // itself would just hit this same error again, so get a fresh download.
        startUpdate();
        return;
      }
      setError(appError);
      setFailedStep('install');
      setPhase('error');
    });
  }, [startUpdate]);

  const retry = useCallback(() => {
    if (failedStep === 'install') install();
    else startUpdate();
  }, [failedStep, install, startUpdate]);

  const later = useCallback(() => {
    api.discardAppUpdate().catch((e) => console.warn('[updater] could not discard the pending update:', e));
    setPhase('idle');
    setDismissed(true);
  }, []);

  const openNotes = useCallback(() => {
    if (!info?.releaseUrl) return;
    setNotesError(null);
    openReleaseUrl(info.releaseUrl).catch((e) => {
      console.warn('[updater] could not open the release page:', e);
      setNotesError(toAppError(e));
    });
  }, [info]);

  const dismiss = useCallback(() => setDismissed(true), []);

  return {
    currentVersion,
    info,
    phase,
    progress,
    error,
    notesError,
    dismissed,
    checkNow,
    startUpdate,
    install,
    retry,
    later,
    dismiss,
    openNotes,
  };
}
