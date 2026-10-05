import { useT } from '../i18n/LanguageContext';
import { ReportProblemLink } from './ReportProblemLink';
import { ALERT_ICON } from './icons';

export function UnexpectedErrorToast({ message, onClose }: { message: string; onClose: () => void }) {
  const t = useT();
  return (
    <div
      role="alert"
      className="fixed bottom-4 right-4 z-50 max-w-[420px] px-3.5 py-2.5 rounded-[4px] border border-[var(--line)] border-l-4 border-l-[var(--crit)] bg-[var(--panel)] shadow-[var(--shadow)] text-[var(--ink)]"
    >
      <div className="flex items-center gap-1.5 text-body font-semibold">
        <span className="text-[var(--crit)]">{ALERT_ICON}</span>
        {t('unexpectedErrorTitle')}
      </div>
      <div className="mt-0.5 text-small text-[var(--ink-2)]">{message} {t('unexpectedErrorText')}</div>
      <div className="mt-1.5 flex gap-3 text-small">
        <ReportProblemLink />
        <button type="button" onClick={onClose} className="text-[var(--ink-3)] cursor-pointer bg-transparent border-0 p-0">
          {t('bugReportClose')}
        </button>
      </div>
    </div>
  );
}
