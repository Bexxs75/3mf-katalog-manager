import { useLanguage, useT } from '../i18n/LanguageContext';
import type { UpdaterView } from '../hooks/useUpdater';
import { UpdateStateBody, updateDisplayState } from './UpdateToast';

/** Settings > Info tab: version, update box (same content as the toast, inline) and recheck. */
export function UpdatePanel({ view }: { view: UpdaterView }) {
  const t = useT();
  const { language } = useLanguage();
  const state = updateDisplayState(view);

  return (
    <>
      <div className="text-center mb-3">
        <div className="text-[13.5px] font-bold">3MF Katalog Manager</div>
        <div className="mt-0.5 font-mono-ui text-[10.5px] text-[var(--ink-3)]">
          {t('infoAppVersionLabel').replace('{version}', view.info?.currentVersion ?? '')}
        </div>
        {state ? (
          <div
            className="mt-2.5 p-2.5 rounded-[5px] text-left"
            style={{ background: 'var(--good-soft, var(--accent-soft))', border: '1px solid var(--line)' }}
          >
            <UpdateStateBody view={view} state={state} />
          </div>
        ) : (
          <div className="mt-2 font-mono-ui text-[10.5px] text-[var(--ink-3)]">{t('infoUpToDateLabel')}</div>
        )}
      </div>
      <button
        onClick={view.checkNow}
        disabled={view.phase === 'checking'}
        className={`h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] ${
          view.phase === 'checking' ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]'
        }`}
      >
        {view.phase === 'checking' ? t('infoCheckingForUpdate') : t('infoCheckForUpdateButton')}
      </button>
      {view.info?.lastUpdate && (
        <div className="mt-2 font-mono-ui text-[10.5px] text-[var(--ink-3)] text-center">
          {t('updateLastInfo')
            .replace(
              '{date}',
              new Date(view.info.lastUpdate.date).toLocaleDateString(language, {
                day: '2-digit',
                month: '2-digit',
                year: 'numeric',
              }),
            )
            .replace('{file}', view.info.lastUpdate.backupFile)}
        </div>
      )}
    </>
  );
}
