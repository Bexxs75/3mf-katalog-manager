import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { openDataFolder, openLogFolder } from '../lib/api/diagnostics';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from './ErrorText';

// Two buttons because on Windows and macOS the logs live in a different system
// folder than the catalog and the backups taken before updates.
export function InfoFolders() {
  const t = useT();
  const [error, setError] = useState<AppError | null>(null);
  const mountedRef = useRef(true);

  useEffect(
    () => () => {
      mountedRef.current = false;
    },
    [],
  );

  const open = (action: () => Promise<void>) => {
    setError(null);
    action().catch((e) => mountedRef.current && setError(toAppError(e)));
  };

  const buttons = [
    { label: t('infoOpenDataFolder'), hint: t('infoOpenDataFolderHint'), action: openDataFolder },
    { label: t('infoOpenLogFolder'), hint: t('infoOpenLogFolderHint'), action: openLogFolder },
  ];

  return (
    <div className="mt-3 pt-3 border-t border-[var(--line)]">
      <div className="ui-label text-[var(--ink-3)]">{t('infoFoldersHeading')}</div>
      <div className="mt-2 flex flex-col gap-1.5">
        {buttons.map(({ label, hint, action }) => (
          <button
            key={label}
            type="button"
            onClick={() => open(action)}
            className="min-h-8 py-1.5 px-3 flex flex-wrap items-center justify-between gap-x-2 gap-y-0.5 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            <span className="shrink-0 whitespace-nowrap">{label}</span>
            <span className="min-w-0 break-words font-normal text-caption text-[var(--ink-3)]">{hint}</span>
          </button>
        ))}
      </div>
      {error && (
        <p className="mt-1 text-caption leading-[1.4] text-[var(--warn)]">
          <ErrorText error={error} />
        </p>
      )}
    </div>
  );
}
