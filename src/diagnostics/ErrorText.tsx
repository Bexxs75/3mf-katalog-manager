import type { AppError } from '../lib/errors';
import { ReportProblemLink } from './ReportProblemLink';
import { useFormatError } from './useFormatError';

/** Error message plus "Report problem" for unexpected errors; the caller keeps its own container styling. */
export function ErrorText({ error }: { error: AppError | null }) {
  const formatError = useFormatError();
  if (!error) return null;
  return (
    <>
      <span>{formatError(error)}</span>
      {error.unexpected && <ReportProblemLink />}
    </>
  );
}
