import { useT } from '../i18n/LanguageContext';
import type { AppError } from '../lib/errors';
import { ReportProblemLink } from './ReportProblemLink';

/** Error message plus "Report problem" for unexpected errors; the caller keeps its own container styling. */
export function ErrorText({ error }: { error: AppError | null }) {
  const t = useT();
  if (!error) return null;
  const message = error.message === 'container' ? t('containerUpdates')
    : error.message === 'containerExternalUnavailable' || error.message === 'containerSlicerUnavailable' || error.message === 'containerFileManagerUnavailable' ? t(error.message)
    : error.message === 'imageUploadTooLarge' || error.message === 'imageUploadUnsupported' || error.message === 'imageUploadUnreadable'
    ? t(error.message)
    : error.message === 'importActive' ? t('importActiveError') : error.message;
  return (
    <>
      <span>{message}</span>
      {error.unexpected && <ReportProblemLink />}
    </>
  );
}
