import { useEffect } from 'react';
import { useT } from '../i18n/LanguageContext';

interface Props {
  imported: number;
  duplicates: number;
  onClose: () => void;
}

export function ImportSummaryBanner({ imported, duplicates, onClose }: Props) {
  const t = useT();

  useEffect(() => {
    const timer = setTimeout(onClose, 5000);
    return () => clearTimeout(timer);
  }, [onClose]);

  return (
    <div className="fixed bottom-4 right-4 z-50 flex items-center gap-2.5 px-3.5 py-2.5 rounded-[4px] border border-[var(--line)] bg-[var(--panel)] shadow-[var(--shadow)] text-[var(--font-size-title)] text-[var(--ink)]">
      <span>
        {t('importSummaryText')
          .replace('{imported}', String(imported))
          .replace('{duplicates}', String(duplicates))}
      </span>
      <span
        onClick={onClose}
        className="w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[var(--font-size-meta)] text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
      >
        ✕
      </span>
    </div>
  );
}
