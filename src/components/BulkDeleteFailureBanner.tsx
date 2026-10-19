import { useFormatCount, useT } from '../i18n/LanguageContext';
import { displayName } from '../lib/displayName';
import { Icon } from './Icon';

interface Props {
  names: string[];
  onClose: () => void;
}

// A large selection can fail wholesale (for example a read-only folder); the
// message stays readable by naming only the first few files.
const MAX_NAMES = 3;

/**
 * Persistent notice after a bulk delete left files behind. It stays until
 * dismissed because the user would otherwise assume everything was removed.
 */
export function BulkDeleteFailureBanner({ names, onClose }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  if (names.length === 0) return null;
  let list = names.slice(0, MAX_NAMES).map(displayName).join(', ');
  if (names.length > MAX_NAMES) {
    list += ' ' + t('bulkDeleteFailedMore').replace('{count}', String(names.length - MAX_NAMES));
  }
  return (
    <div role="alert" className="pointer-events-auto max-w-[min(90vw,480px)] flex items-start gap-2.5 px-3.5 py-2.5 rounded-[4px] border border-red-400 bg-[var(--panel)] shadow-[var(--shadow)] text-[length:var(--font-size-title)] text-red-400">
      <p className="flex-1 min-w-0 break-words">{formatCount(t('bulkDeleteFailed'), names.length).replace('{names}', () => list)}</p>
      <button
        type="button"
        onClick={onClose}
        aria-label={t('bulkDeleteFailedDismiss')}
        className="flex-none w-5 h-5 grid place-items-center rounded-full cursor-pointer text-[var(--ink-3)] hover:bg-[var(--panel-2)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]"
      >
        <Icon name="close" size={14} />
      </button>
    </div>
  );
}
