import { useT } from '../i18n/LanguageContext';
import { ErrorText } from '../diagnostics/ErrorText';
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
  'h-7 px-2.5 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12px] font-semibold cursor-pointer';
const secondaryBtn = 'h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] cursor-pointer';
const linkBtn = 'text-[12px] font-semibold text-[var(--accent)] hover:underline cursor-pointer bg-transparent border-0 p-0';

/**
 * Content for the current update state - shared between the toast (a floating
 * popup) and the Info tab panel (an inline box), which show the same text and
 * actions in different containers.
 */
export function UpdateStateBody({ view, state }: { view: UpdaterView; state: UpdateDisplayState }) {
  const t = useT();
  const version = view.info?.availableVersion ?? '';

  if (state === 'available') {
    return (
      <>
        <div className="text-[13px] font-semibold">{t('updateAvailableTitle').replace('{version}', version)}</div>
        <div className="mt-1 text-[12px] text-[var(--ink-2)] leading-snug">
          {t('updateAvailableBody').replace('{current}', view.info?.currentVersion ?? '')}
        </div>
        <div className="mt-2 flex items-center gap-3 flex-wrap">
          {view.info?.canInstall ? (
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
      </>
    );
  }

  if (state === 'downloading') {
    const progress = view.progress;
    const doneMb = toMb(progress?.downloaded ?? 0);
    const totalMb = progress?.total != null ? toMb(progress.total) : null;
    const percent = progress?.total ? Math.round((progress.downloaded / progress.total) * 100) : 0;
    return (
      <>
        <div className="text-[13px] font-semibold">{t('updateDownloadingTitle').replace('{version}', version)}</div>
        <div
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent}
          className="mt-2 h-2 w-full rounded-full bg-[var(--panel-2)] border border-[var(--line)] overflow-hidden"
        >
          <div className="h-full rounded-full bg-[var(--accent)]" style={{ width: `${percent}%` }} />
        </div>
        <div className="mt-1 font-mono-ui text-[11px] text-[var(--ink-3)]">
          {totalMb !== null
            ? t('updateDownloadingProgress')
                .replace('{done}', String(doneMb))
                .replace('{total}', String(totalMb))
                .replace('{percent}', String(percent))
            : t('updateDownloadingProgressUnknown').replace('{done}', String(doneMb))}
        </div>
        <div className="mt-1 text-[12px] text-[var(--ink-2)]">{t('updateDownloadingHint')}</div>
      </>
    );
  }

  if (state === 'ready') {
    return (
      <>
        <div className="text-[13px] font-semibold">{t('updateReadyTitle').replace('{version}', version)}</div>
        <div className="mt-1 text-[12px] text-[var(--ink-2)] leading-snug">{t('updateReadyBody')}</div>
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
        <div className="text-[13px] font-semibold">{t('updateInstallingTitle')}</div>
        <div className="mt-1 font-mono-ui text-[11px] text-[var(--ink-3)]">{`update-backups/catalog-vor-${version}.db`}</div>
        <div className="mt-1 text-[12px] text-[var(--ink-2)]">{t('updateInstallingBody')}</div>
      </>
    );
  }

  return (
    <>
      <div className="text-[13px] font-semibold">{t('updateFailedTitle')}</div>
      <div className="mt-0.5 text-[12px] text-[var(--ink-2)]">
        <ErrorText error={view.error} />
      </div>
      <div className="mt-0.5 text-[12px] text-[var(--ink-2)]">{t('updateFailedBody')}</div>
      <div className="mt-2">
        <button onClick={view.retry} className={primaryBtn}>
          {t('updateRetryButton')}
        </button>
      </div>
    </>
  );
}

export function UpdateToast({ view }: { view: UpdaterView }) {
  const state = updateDisplayState(view);
  if (!state) return null;
  if (state === 'available' && view.dismissed) return null;
  const isError = state === 'error';

  return (
    <div
      role={isError ? 'alert' : undefined}
      className={`fixed bottom-4 right-4 z-50 max-w-[400px] px-3.5 py-2.5 rounded-[4px] border border-[var(--line)] ${
        isError ? 'border-l-4 border-l-[var(--crit)]' : ''
      } bg-[var(--panel)] shadow-[var(--shadow)] text-[var(--ink)]`}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex-1 min-w-0">
          <UpdateStateBody view={view} state={state} />
        </div>
        {(state === 'available' || state === 'error') && (
          <span
            onClick={view.dismiss}
            className="w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[length:var(--font-size-meta)] text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
          >
            ✕
          </span>
        )}
      </div>
    </div>
  );
}
