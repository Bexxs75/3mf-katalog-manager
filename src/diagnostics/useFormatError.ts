import { useCallback } from 'react';
import { useT } from '../i18n/LanguageContext';
import type { AppError } from '../lib/errors';
import { formatAppError } from '../lib/errorMessage';

/** The error text in the chosen language, for places that show an error without `ErrorText`. */
export function useFormatError() {
  const t = useT();
  return useCallback((error: Pick<AppError, 'message' | 'params'>) => formatAppError(t, error), [t]);
}
