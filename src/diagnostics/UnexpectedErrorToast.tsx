import { useT } from '../i18n/LanguageContext';
import { ReportProblemLink } from './ReportProblemLink';

export function UnexpectedErrorToast({ message, onClose }: { message: string; onClose: () => void }) {
  const t = useT();
  return (
    <div
      role="alert"
      className="fixed bottom-4 right-4 z-50 max-w-[420px] px-3.5 py-2.5 rounded-[4px] border border-[var(--line)] border-l-4 border-l-[var(--crit)] bg-[var(--panel)] shadow-[var(--shadow)] text-[var(--ink)]"
    >
      <div className="text-[13px] font-semibold">{t('unexpectedErrorTitle')}</div>
      <div className="mt-0.5 text-[12px] text-[var(--ink-2)]">{message} {t('unexpectedErrorText')}</div>
      <div className="mt-1.5 flex gap-3 text-[12px]">
        <ReportProblemLink />
        <button type="button" onClick={onClose} className="text-[var(--ink-3)] cursor-pointer bg-transparent border-0 p-0">
          {t('bugReportClose')}
        </button>
      </div>
    </div>
  );
}
