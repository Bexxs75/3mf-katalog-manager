import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { Icon } from './Icon';
import { useT } from '../i18n/LanguageContext';
import { ErrorText } from '../diagnostics/ErrorText';
import { discardAppUpdate } from '../lib/api/updater';
import type { UpdaterView } from '../hooks/useUpdater';

export type UpdateDisplayState = 'available' | 'downloading' | 'ready' | 'installing' | 'error';

/** Which state the toast/panel currently shows, or null when there is nothing to say. */
export function updateDisplayState(view: UpdaterView): UpdateDisplayState | null {
  if (view.phase === 'downloading' || view.phase === 'ready' || view.phase === 'installing' || view.phase === 'error') {
    return view.phase;
  }
  return view.info?.availableVersion ? 'available' : null;
}

// The backend reports bytes; the UI only ever shows whole MB.
function toMb(bytes: number): number {
  return Math.round(bytes / (1024 * 1024));
}

const primaryBtn =
  'hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-7 px-2.5 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-small font-semibold cursor-pointer';
const secondaryBtn = 'hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small cursor-pointer';
const linkBtn = 'text-small font-semibold text-[var(--accent)] hover:underline cursor-pointer bg-transparent border-0 p-0';

/**
 * Content for the current update state - shared between the toast (a floating
 * popup) and the Info tab panel (an inline box), which show the same text and
 * actions for every state except "available": the panel's box only shows the
 * title and actions there (no body copy), so `variant` trims that one case.
 */
export function UpdateStateBody({
  view,
  state,
  variant = 'toast',
}: {
  view: UpdaterView;
  state: UpdateDisplayState;
  variant?: 'toast' | 'panel';
}) {
  const t = useT();
  const version = view.info?.availableVersion ?? '';
  const backupNotice = t('updateBackupLocation')
    .replace('{from}', view.currentVersion || view.info?.currentVersion || '')
    .replace('{to}', version);

  if (state === 'available') {
    const canInstall = view.info?.canInstall ?? false;
    return (
      <>
        <div
          className="text-body font-semibold"
          style={variant === 'panel' ? { color: 'var(--good, var(--accent))' } : undefined}
        >
          {t(version.includes('-') ? 'updatePrereleaseTitle' : 'updateAvailableTitle').replace('{version}', version)}
        </div>
        {variant === 'toast' && (
          <div className="mt-1 text-small text-[var(--ink-2)] leading-snug">
            {t('updateAvailableBody').replace('{current}', view.currentVersion)}
          </div>
        )}
        {canInstall && <p className="mt-1 text-small text-[var(--ink-2)]">{backupNotice}</p>}
        <div className="mt-2 flex items-center gap-3 flex-wrap">
          {canInstall ? (
            <button onClick={view.startUpdate} className={primaryBtn}>
              {t('updateNowButton')}
            </button>
          ) : (
            <button onClick={view.openNotes} className={primaryBtn}>
              {t('updateDownloadPageButton')}
            </button>
          )}
          <button onClick={view.openNotes} className={linkBtn}>
            {t('updateWhatsNew')}
          </button>
        </div>
        {/* Opening the download page is the only action offered when self-install
            isn't possible, so a failure to open it must be visible right here. */}
        {!canInstall && view.notesError && (
          <div className="mt-1 text-small text-[var(--ink-2)]">
            <ErrorText error={view.notesError} />
          </div>
        )}
      </>
    );
  }

  if (state === 'downloading') {
    const progress = view.progress;
    const doneMb = toMb(progress?.downloaded ?? 0);
    const totalMb = progress?.total != null ? toMb(progress.total) : null;
    const percent = progress?.total ? Math.round((progress.downloaded / progress.total) * 100) : null;
    return (
      <>
        <div className="text-body font-semibold">{t('updateDownloadingTitle').replace('{version}', version)}</div>
        <div
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          {...(percent !== null ? { 'aria-valuenow': percent } : {})}
          className="mt-2 h-2 w-full rounded-full bg-[var(--panel-2)] border border-[var(--line)] overflow-hidden"
        >
          <div
            className={`h-full rounded-full bg-[var(--accent)] ${percent === null ? 'animate-pulse' : ''}`}
            style={{ width: percent !== null ? `${percent}%` : '100%' }}
          />
        </div>
        <div className="mt-1 font-medium tabular-nums text-caption text-[var(--ink-3)]">
          {totalMb !== null
            ? t('updateDownloadingProgress')
                .replace('{done}', String(doneMb))
                .replace('{total}', String(totalMb))
                .replace('{percent}', String(percent))
            : t('updateDownloadingProgressUnknown').replace('{done}', String(doneMb))}
        </div>
        <div className="mt-1 text-small text-[var(--ink-2)]">{t('updateDownloadingHint')}</div>
      </>
    );
  }

  if (state === 'ready') {
    return (
      <>
        <div className="text-body font-semibold">{t('updateReadyTitle').replace('{version}', version)}</div>
        <div className="mt-1 text-small text-[var(--ink-2)] leading-snug">{t('updateReadyBody')}</div>
        <p className="mt-1 text-small text-[var(--ink-2)]">{backupNotice}</p>
        <div className="mt-2 flex gap-2">
          <button onClick={view.install} className={primaryBtn}>
            {t('updateRestartButton')}
          </button>
          <button onClick={view.later} className={secondaryBtn}>
            {t('updateLaterButton')}
          </button>
        </div>
      </>
    );
  }

  if (state === 'installing') {
    return (
      <>
        <div className="text-body font-semibold">{t('updateInstallingTitle')}</div>
        <p className="mt-1 text-small text-[var(--ink-2)]">{backupNotice}</p>
        <div className="mt-1 text-small text-[var(--ink-2)]">{t('updateInstallingBody')}</div>
      </>
    );
  }

  return (
    <>
      <div className="text-body font-semibold">{t('updateFailedTitle')}</div>
      <div className="mt-0.5 text-small text-[var(--ink-2)]">
        <ErrorText error={view.error} />
      </div>
      <div className="mt-0.5 text-small text-[var(--ink-2)]">{t('updateFailedBody')}</div>
      <div className="mt-2">
        <button onClick={view.retry} className={primaryBtn}>
          {t('updateRetryButton')}
        </button>
      </div>
    </>
  );
}

export function UpdateToast({ view }: { view: UpdaterView }) {
  const { container } = useRuntimeEnvironment();
  const t = useT();
  const state = updateDisplayState(view);
  if (container || !state) return null;
  // "available" and "error" are the only states with a close button; once
  // dismissed the toast disappears even though the failed update (still
  // actionable from the Info panel) stays in `phase === 'error'` otherwise.
  if (view.dismissed && (state === 'available' || state === 'error')) return null;
  const isError = state === 'error';

  // Dismissing an "available" offer just hides the toast - nothing was downloaded
  // yet. Dismissing an error can leave an already-downloaded ~100 MB package sitting
  // in memory (e.g. a failed backup kept it for a install retry); discard it here so
  // it doesn't linger just because the user closed the toast instead of retrying. If
  // there was nothing pending, the backend call is a no-op.
  const handleDismiss = () => {
    view.dismiss();
    if (isError) {
      discardAppUpdate().catch((e) => console.warn('[updater] could not discard the pending update:', e));
    }
  };

  return (
    <div
      role={isError ? 'alert' : undefined}
      className={`fixed bottom-4 right-4 z-50 max-w-[400px] px-3.5 py-2.5 rounded-[4px] border border-[var(--line)] ${
        isError ? 'border-l-4 border-l-[var(--crit)]' : ''
      } bg-[var(--panel)] shadow-[var(--shadow)] text-[var(--ink)]`}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex-1 min-w-0">
          <UpdateStateBody view={view} state={state} variant="toast" />
        </div>
        {(state === 'available' || state === 'error') && (
          <button
            type="button"
            onClick={handleDismiss}
            aria-label={t('updateDismissAria')}
            className="w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[length:var(--font-size-meta)] text-[var(--ink-3)] hover:bg-[var(--panel-2)] bg-transparent border-0 p-0"
          >
            <Icon name="close" size={14} />
          </button>
        )}
      </div>
    </div>
  );
}
