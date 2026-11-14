import { displayName } from '../lib/displayName';
import { Icon } from './Icon';
import { useT } from '../i18n/LanguageContext';
import type { DropTarget } from '../lib/externalDrop';

interface Props {
  target: Extract<DropTarget, { kind: 'invalid' }>;
  onDismiss: () => void;
}

/** Shown when files are dropped on a folder that is no target; nothing was imported or moved. */
export function DropRejectedNotice({ target, onDismiss }: Props) {
  const t = useT();
  const text = target.reason === 'missing'
    ? t('dzTargetMissing').replace('{name}', displayName(target.folder.name))
    : t('dzTargetCatalogRoot');
  return (
    <div role="alert" className="flex items-start gap-2 rounded-[8px] border border-[var(--line)] border-l-[3px] border-l-[var(--warn)] bg-[var(--panel)] px-3 py-2 text-small text-[var(--ink)]">
      <span className="text-[var(--warn)] shrink-0 mt-0.5 flex"><Icon name="warning" size={14} /></span>
      <span className="flex-1">{text}</span>
      <button type="button" onClick={onDismiss}
        className="shrink-0 min-h-7 px-2.5 rounded-[4px] border border-[var(--line-strong)] text-small font-semibold text-[var(--ink-2)] hover:border-[var(--accent)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
        {t('dzClose')}
      </button>
    </div>
  );
}
