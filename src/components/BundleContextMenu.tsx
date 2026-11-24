import { useEffect, useRef } from 'react';
import { useCanEditMetadata } from '../lib/webSession';
import { useImportLock } from '../hooks/ImportLockContext';
import { useT } from '../i18n/LanguageContext';
import { ContextMenuFrame } from './ContextMenu';
import { WebFileActions } from './WebFileActions';

interface Props {
  x: number;
  y: number;
  /** Living members of the bundle: the number the entries name. */
  count: number;
  allPrinted: boolean;
  allFavorite: boolean;
  /** Variants are alternatives, so the entry does not queue them all: it opens the question which one. */
  queueChoice: boolean;
  /** Trashing needs the right to manage files, which the web version may lack. */
  canDelete: boolean;
  onClose: () => void;
  onEdit: () => void;
  onTogglePrinted: () => void;
  onQueueAll: () => void;
  onToggleFavorite: () => void;
  onDissolve: () => void;
  onDelete: () => void;
  /** Desktop: opens the "Move to ..." dialog for the living members. Omitted when the caller cannot move files. */
  onMove?: () => void;
  /** Web: the living members, moved through the file operations dialog. */
  moveFileIds?: string[];
  name?: string;
  /** F6 on an open bundle card: runs the move entry right away. */
  autoMove?: boolean;
}

const ITEM = 'w-full text-left px-3 py-2 text-[length:var(--font-size-title)] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)] disabled:opacity-50';

/** Menu of a bundle card: every action works on all living members, and the text says how many. */
export function BundleContextMenu({ x, y, count, allPrinted, allFavorite, queueChoice, canDelete, onClose, onEdit, onTogglePrinted, onQueueAll, onToggleFavorite, onDissolve, onDelete, onMove, moveFileIds, name, autoMove }: Props) {
  const t = useT();
  const groupRef = useRef<HTMLDivElement>(null);
  useEffect(() => { if (autoMove) groupRef.current?.querySelector<HTMLButtonElement>('[aria-keyshortcuts="F6"]')?.click(); }, []);
  const canEdit = useCanEditMetadata();
  const { lockProps } = useImportLock();
  if (!canEdit) return null;
  const entry = (label: string, action: () => void, extra: { critical?: boolean; locked?: boolean } = {}) => (
    <button key={label} {...(extra.locked ? lockProps : {})} className={extra.critical ? ITEM.replace('text-[var(--ink)]', 'text-[var(--crit)]') : ITEM}
      onClick={() => { action(); onClose(); }}>{label}</button>
  );
  const named = (key: 'bundleMarkAllPrinted' | 'bundleMarkAllNotPrinted' | 'bundleQueueAll' | 'bundleFavoriteAll' | 'bundleUnfavoriteAll') =>
    t(key).replace('{count}', String(count));
  return (
    <ContextMenuFrame x={x} y={y} onClose={onClose}>
      <div role="group" ref={groupRef} aria-label={t('bundleMenuLabel')}>
        {entry(t('bundleEdit'), onEdit)}
        {entry(named(allPrinted ? 'bundleMarkAllNotPrinted' : 'bundleMarkAllPrinted'), onTogglePrinted)}
        {entry(queueChoice ? t('bundleQueueOpen') : named('bundleQueueAll'), onQueueAll)}
        {entry(named(allFavorite ? 'bundleUnfavoriteAll' : 'bundleFavoriteAll'), onToggleFavorite)}
        {moveFileIds ? <WebFileActions fileIds={moveFileIds} name={name} onDone={onClose} menuAction="move" menuLabel={t('moveContextLabel')} menuClassName={ITEM} menuShortcut="F6" />
          : onMove && entry(t('moveContextLabel'), onMove, { locked: true })}
        <div className="border-t border-[var(--line)]" />
        {entry(t('bundleDissolve'), onDissolve)}
        {canDelete && entry(t('bundleDeleteEntry'), onDelete, { critical: true, locked: true })}
      </div>
    </ContextMenuFrame>
  );
}
