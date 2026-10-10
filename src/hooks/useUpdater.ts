import { useRuntimeEnvironment } from './useRuntimeEnvironment';
import { useCallback, useEffect, useRef, useState } from 'react';
import * as api from '../lib/api/updater';
import { getAppVersion, isPreviewBuild, openReleaseUrl } from '../lib/api/update';
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
 * release went up in between), and the notes link must follow it. All preview
 * builds share one release regardless of their `x.y.z-n` version, so `preview`
 * routes to the fixed preview tag instead of a per-version one. */
function releasePage(version: string, preview: boolean): string {
  return preview
    ? 'https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/preview'
    : `https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v${version}`;
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
  /** Null until the build kind is known, so channel controls cannot flash on preview builds. */
  preview: boolean | null;
}

export function isUpdateCheckDisabled(phase: UpdatePhase): boolean {
  return phase === 'checking' || BUSY_PHASES.includes(phase);
}

export function useUpdater(): UpdaterView {
  const { container } = useRuntimeEnvironment();
  const [currentVersion, setCurrentVersion] = useState('');
  const [info, setInfo] = useState<api.UpdateInfo | null>(null);
  const [phase, setPhase] = useState<UpdatePhase>('idle');
  const [progress, setProgress] = useState<api.DownloadProgress | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [notesError, setNotesError] = useState<AppError | null>(null);
  const [dismissed, setDismissed] = useState(false);
  const [failedStep, setFailedStep] = useState<FailedStep | null>(null);
  const [preview, setPreview] = useState<boolean | null>(null);

  // Read without adding `phase` to checkNow's dependencies, which would recreate
  // it (and re-fire the mount effect below) on every phase change.
  const phaseRef = useRef(phase);
  phaseRef.current = phase;

  // Same reasoning as phaseRef: startUpdate below has no deps, so it reads this
  // instead of closing over a stale `preview` from the render it was created in.
  const previewRef = useRef(preview ?? false);
  previewRef.current = preview ?? false;

  // Plain (non-state) in-flight guards: a state-based check would still let a
  // second call through when both happen before React re-renders (e.g. two
  // clicks in the same tick), since the guard would still read the old phase.
  const downloadBusyRef = useRef(false);
  const installBusyRef = useRef(false);

  const checkNow = useCallback(() => {
    if (container) return;
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
  }, [container]);

  useEffect(() => {
    // Independent of the update check (which may be slow or offline), so the
    // running version is visible right away.
    getAppVersion()
      .then(setCurrentVersion)
      .catch((e) => console.warn('[updater] could not determine the app version:', e));
    isPreviewBuild()
      .then(setPreview)
      .catch((e) => console.warn('[updater] could not determine the build kind:', e));
    checkNow();
  }, [checkNow]);

  const startUpdate = useCallback(() => {
    if (container) return;
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
        setInfo((prev) =>
          prev ? { ...prev, availableVersion: version, releaseUrl: releasePage(version, previewRef.current) } : prev,
        );
        setPhase('ready');
      })
      .catch((e) => {
        downloadBusyRef.current = false;
        setError(toAppError(e));
        setFailedStep('download');
        setPhase('error');
      });
  }, [container]);

  const install = useCallback(() => {
    if (container) return;
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
  }, [startUpdate, container]);

  const retry = useCallback(() => {
    if (failedStep === 'install') install();
    else startUpdate();
  }, [failedStep, install, startUpdate]);

  const later = useCallback(() => {
    if (container) return;
    api.discardAppUpdate().catch((e) => console.warn('[updater] could not discard the pending update:', e));
    setPhase('idle');
    setDismissed(true);
  }, [container]);

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
    preview,
  };
}
