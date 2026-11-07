import { useT } from '../i18n/LanguageContext';
import { useCanEditMetadata } from '../lib/webSession';
export function DragGrip({ className = '' }: { className?: string }) {
  const t = useT();
  const canEdit = useCanEditMetadata();
  if (!canEdit) return null;
  // The non-interactive span owns the tooltip; mouse events bubble to the card.
  return (
    <span data-drag-grip aria-hidden="true" title={t('dgMoveHint')} className={`w-4 h-4 shrink-0 grid place-items-center rounded border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-3)] opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 ${className}`}>
      <svg className="pointer-events-none" viewBox="0 0 10 10" width="11" height="11" fill="currentColor">
        <circle cx="3" cy="2" r="1" /><circle cx="7" cy="2" r="1" />
        <circle cx="3" cy="5" r="1" /><circle cx="7" cy="5" r="1" />
        <circle cx="3" cy="8" r="1" /><circle cx="7" cy="8" r="1" />
      </svg>
    </span>
  );
}
