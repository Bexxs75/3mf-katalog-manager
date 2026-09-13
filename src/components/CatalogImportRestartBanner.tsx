import { useT } from '../i18n/LanguageContext';

interface Props {
  onClose: () => void;
}

export function CatalogImportRestartBanner({ onClose }: Props) {
  const t = useT();

  return (
    <div className="fixed bottom-4 right-4 z-50 flex items-center gap-2.5 px-3.5 py-2.5 rounded-[4px] border border-[var(--accent)] bg-[var(--panel)] shadow-[var(--shadow)] text-[length:var(--font-size-title)] text-[var(--ink)] max-w-[360px]">
      <span>{t('importCatalogRestartHint')}</span>
      <span
        onClick={onClose}
        className="w-4 h-4 flex-none grid place-items-center rounded-full cursor-pointer text-[length:var(--font-size-meta)] text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
      >
        ✕
      </span>
    </div>
  );
}
