import { useCallback, useEffect, useState } from 'react';
import * as api from '../lib/api/updater';
import { openReleaseUrl } from '../lib/api/update';
import { toAppError, type AppError } from '../lib/errors';

export type UpdatePhase = 'idle' | 'checking' | 'downloading' | 'ready' | 'installing' | 'error';

/** Which action to retry after a failure - the backend keeps a downloaded update on a
 * failed install (e.g. a failed backup), so retrying must not re-download it. */
type FailedStep = 'download' | 'install';

export interface UpdaterView {
  info: api.UpdateInfo | null;
  phase: UpdatePhase;
  progress: api.DownloadProgress | null;
  error: AppError | null;
  dismissed: boolean;
  checkNow: () => void;
  startUpdate: () => void;
  install: () => void;
  retry: () => void;
  later: () => void;
  dismiss: () => void;
  openNotes: () => void;
}

export function useUpdater(): UpdaterView {
  const [info, setInfo] = useState<api.UpdateInfo | null>(null);
  const [phase, setPhase] = useState<UpdatePhase>('idle');
  const [progress, setProgress] = useState<api.DownloadProgress | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [dismissed, setDismissed] = useState(false);
  const [failedStep, setFailedStep] = useState<FailedStep | null>(null);

  const checkNow = useCallback(() => {
    setDismissed(false);
    setPhase('checking');
    api
      .checkAppUpdate()
      .then(setInfo)
      // The backend never rejects for network problems; anything else is logged there.
      .catch(() => {})
      .finally(() => setPhase((p) => (p === 'checking' ? 'idle' : p)));
  }, []);

  useEffect(() => {
    checkNow();
  }, [checkNow]);

  const startUpdate = useCallback(() => {
    setError(null);
    setProgress({ downloaded: 0, total: null });
    setPhase('downloading');
    api
      .downloadAppUpdate(setProgress)
      .then(() => setPhase('ready'))
      .catch((e) => {
        setError(toAppError(e));
        setFailedStep('download');
        setPhase('error');
      });
  }, []);

  const install = useCallback(() => {
    setPhase('installing');
    // On success the app restarts and this promise never settles.
    api.installAppUpdate().catch((e) => {
      setError(toAppError(e));
      setFailedStep('install');
      setPhase('error');
    });
  }, []);

  const retry = useCallback(() => {
    if (failedStep === 'install') install();
    else startUpdate();
  }, [failedStep, install, startUpdate]);

  const later = useCallback(() => {
    api.discardAppUpdate().catch(() => {});
    setPhase('idle');
    setDismissed(true);
  }, []);

  const openNotes = useCallback(() => {
    if (info?.releaseUrl) openReleaseUrl(info.releaseUrl).catch(() => {});
  }, [info]);

  return {
    info,
    phase,
    progress,
    error,
    dismissed,
    checkNow,
    startUpdate,
    install,
    retry,
    later,
    dismiss: () => setDismissed(true),
    openNotes,
  };
}
