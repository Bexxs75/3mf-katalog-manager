import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import type { AppError } from '../lib/errors';
import { UpdateChannelControl } from './UpdateChannelControl';
import { useLanguage, useT } from '../i18n/LanguageContext';
import type { Language } from '../i18n/types';
import type { UpdaterView } from '../hooks/useUpdater';
import { isUpdateCheckDisabled } from '../hooks/useUpdater';
import { UpdateStateBody, updateDisplayState } from './UpdateToast';

/**
 * The backend stores the date as a local `YYYY-MM-DD` string (no time, no
 * offset); parsing that via `new Date(iso)` reads it as UTC midnight, which
 * would show the previous day anywhere west of UTC. Building the date from
 * the local y/m/d parts instead keeps the day the backend actually meant.
 */
function formatLastUpdateDate(iso: string, language: Language): string {
  const [year, month, day] = iso.split('-').map(Number);
  return new Date(year, month - 1, day).toLocaleDateString(language, {
    day: '2-digit',
    month: '2-digit',
    year: 'numeric',
  });
}

/** Settings > Info tab: version, update box (same content as the toast, inline) and recheck. */
export function UpdatePanel({ view, onExport, exportError }: { view: UpdaterView; onExport?: () => Promise<boolean>; exportError?: AppError | null }) {
  const { container } = useRuntimeEnvironment();
  const t = useT();
  const { language } = useLanguage();
  const state = updateDisplayState(view);

  return (
    <>
      <div className="text-center mb-3">
        {/* Shown in every state (checking, up to date, update available, error, ...) -
            a preview build's Info tab needs to say so regardless of what the updater is doing. */}
        {view.preview && (
          <div className="mb-2 font-medium tabular-nums text-caption text-[var(--ink-3)]">{t('updatePreviewNote')}</div>
        )}
        <div className="text-body font-bold">3MF Katalog Manager</div>
        <div className="mt-0.5 text-caption text-[var(--ink-3)]">
          {t('infoAppVersionLabel').split('{version}').map((part, index) => <span key={index}>{index > 0 && <span className="font-code">{view.currentVersion}</span>}{part}</span>)}
        </div>
        {container ? (
          <p className="mt-3 text-small text-[var(--ink-3)]">
            {t('containerUpdates').split(/(docker compose pull|docker compose up -d)/).map((part, index) =>
              part.startsWith('docker compose ') ? <code key={index}>{part}</code> : part)}
          </p>
        ) : state ? (
          <div
            className="mt-2.5 p-2.5 rounded-[5px] text-left"
            style={{ background: 'var(--panel-2)', border: '1px solid var(--line)' }}
          >
            <UpdateStateBody view={view} state={state} variant="panel" />
          </div>
        ) : (
          // Only claim "up to date" once the check has actually answered
          // (info !== null); before that, just the version is shown.
          view.info && (
            <>
              <div className="mt-2 font-medium tabular-nums text-caption text-[var(--ink-3)]">
                {view.info.checkFailed ? t('infoUpdateCheckFailed') : t('infoUpToDateLabel')}
              </div>
              {view.info.lastUpdate && (
                <div className="mt-1 font-medium tabular-nums text-caption text-[var(--ink-3)]">
                  {t('updateLastInfo')
                    .replace('{date}', formatLastUpdateDate(view.info.lastUpdate.date, language))
                    .replace('{file}', view.info.lastUpdate.backupFile)}
                </div>
              )}
            </>
          )
        )}
      </div>
      {!container && view.preview === false && onExport && <UpdateChannelControl disabled={isUpdateCheckDisabled(view.phase)} onExport={onExport} exportError={exportError} onChanged={view.checkNow} />}
      {!container && <button
        onClick={view.checkNow}
        disabled={isUpdateCheckDisabled(view.phase)}
        className={`h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small ${
          isUpdateCheckDisabled(view.phase) ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]'
        }`}
      >
        {view.phase === 'checking' ? t('infoCheckingForUpdate') : t('infoCheckForUpdateButton')}
      </button>}
    </>
  );
}
