import { isReadOnly } from '../lib/readOnly';
import { useT } from '../i18n/LanguageContext';
import { useDiagnostics } from './DiagnosticsContext';

export function ReportProblemLink() {
  const t = useT();
  const { openBugReport } = useDiagnostics();
  if (isReadOnly()) return null;
  return (
    <button
      type="button"
      onClick={openBugReport}
      className="ml-2 font-semibold text-[var(--accent)] hover:underline cursor-pointer bg-transparent border-0 p-0"
    >
      {t('reportProblemLink')}
    </button>
  );
}
