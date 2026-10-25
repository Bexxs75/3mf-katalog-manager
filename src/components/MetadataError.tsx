import { useSyncExternalStore } from 'react';
import { clearMetadataError, getMetadataError, subscribeMetadataErrors } from '../lib/metadataErrors';
import { useT } from '../i18n/LanguageContext';
import { formatAppError } from '../lib/errorMessage';

export function MetadataError() {
  const error = useSyncExternalStore(subscribeMetadataErrors, getMetadataError);
  const t = useT();
  if (!error) return null;
  return <div role="alert" className="fixed top-2 left-2 right-2 z-[100] border rounded bg-[var(--panel)] text-[var(--crit)] p-3">
    {t('webMetadataError')} {formatAppError(t, { message: error })}
    <button className="ml-3 underline" onClick={clearMetadataError}>{t('webCancel')}</button>
  </div>;
}
