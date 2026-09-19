import { useT } from '../i18n/LanguageContext';

interface Props {
  latestVersion: string;
  onDownload: () => void;
  onDismiss: () => void;
}

export function UpdateAvailableToast({ latestVersion, onDownload, onDismiss }: Props) {
  const t = useT();
  return (
    <div className="fixed bottom-4 right-4 z-50 flex items-center gap-2.5 px-3.5 py-2.5 rounded-[4px] border border-[var(--line)] bg-[var(--panel)] shadow-[var(--shadow)] text-[length:var(--font-size-title)] text-[var(--ink)]">
      <span>{t('updateToastText').replace('{version}', latestVersion)}</span>
      <button
        onClick={onDownload}
        className="h-7 px-2.5 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12px] font-semibold cursor-pointer"
      >
        {t('updateToastDownloadLabel')}
      </button>
      <span
        onClick={onDismiss}
        className="w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[length:var(--font-size-meta)] text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
      >
        ✕
      </span>
    </div>
  );
}
