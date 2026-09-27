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
      <div className="font-mono-ui text-[10px] tracking-[0.14em] text-[var(--ink-3)]">{t('infoFoldersHeading')}</div>
      <div className="mt-2 flex flex-col gap-1.5">
        {buttons.map(({ label, hint, action }) => (
          <button
            key={label}
            type="button"
            onClick={() => open(action)}
            className="h-8 px-3 flex items-center justify-between gap-2 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[12px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            <span className="truncate">{label}</span>
            <span className="truncate font-normal text-[10.5px] text-[var(--ink-3)]">{hint}</span>
          </button>
        ))}
      </div>
      {error && (
        <p className="mt-1 text-[10.5px] leading-[1.4] text-[var(--warn)]">
          <ErrorText error={error} />
        </p>
      )}
    </div>
  );
}
