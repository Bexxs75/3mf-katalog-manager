import { ErrorText } from '../diagnostics/ErrorText';
import { useT } from '../i18n/LanguageContext';
import type { AppError } from '../lib/errors';
import { Icon } from './Icon';

export function ImageUploadError({ error, onDismiss }: { error: AppError | null; onDismiss: () => void }) {
  const t = useT();
  if (!error) return null;
  return <div role="alert" className="flex items-start gap-2 px-3 py-2 text-small text-red-400">
    <p className="flex-1"><ErrorText error={error} /></p>
    <button type="button" onClick={onDismiss} aria-label={t('filamentImageErrorDismiss')}><Icon name="close" size={16} /></button>
  </div>;
}
