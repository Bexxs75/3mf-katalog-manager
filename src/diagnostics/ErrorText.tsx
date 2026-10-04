import { useT } from '../i18n/LanguageContext';
import type { AppError } from '../lib/errors';
import { ReportProblemLink } from './ReportProblemLink';

/** Error message plus "Report problem" for unexpected errors; the caller keeps its own container styling. */
export function ErrorText({ error }: { error: AppError | null }) {
  const t = useT();
  if (!error) return null;
  return (
    <>
      <span>{error.message === 'importActive' ? t('importActiveError') : error.message}</span>
      {error.unexpected && <ReportProblemLink />}
    </>
  );
}
