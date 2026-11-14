import { useEffect, useMemo, useRef, useState } from 'react';
import { useFormatError } from '../diagnostics/useFormatError';
import { useFormatCount, useLanguage, useT } from '../i18n/LanguageContext';
import { formatBytes, formatDate } from '../i18n/format';
import { useImportLock } from '../hooks/ImportLockContext';
import { displayName } from '../lib/displayName';
import { groupDuplicates } from '../lib/duplicateGroups';
import { toAppError } from '../lib/errors';
import { useCanManageFiles } from '../lib/webSession';
import type { ModelFile } from '../types';
import { DuplicateRemoveDialog } from './DuplicateRemoveDialog';

interface Props {
  /** All models of the catalog: this view ignores the filters of the catalog view. */
  models: ModelFile[];
  /** Puts the models in the app trash and resolves with the ids that are still in the catalog afterwards. Omitted: the view only shows. */
  onRemove?: (ids: string[]) => Promise<string[]>;
}

const buttonClass = 'h-8 px-3 rounded-[5px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[length:var(--font-size-control)] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)] disabled:opacity-40 disabled:cursor-not-allowed';
const pillClass = 'inline-flex items-center h-[20px] px-2 rounded-full border text-caption font-semibold';

/**
 * Models with the same content, grouped. Exactly one entry per group is kept (a radio group has no
 * "none" state), the oldest by default; the others are put in the app trash after a confirmation.
 * Files outside the catalog are not part of it, and a model whose file is gone cannot be told apart
 * here (the cleanup suggestions do that), so the trash is the net.
 */
export function DuplicatesView({ models, onRemove }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  const formatError = useFormatError();
  const { language } = useLanguage();
  const { lockProps } = useImportLock();
  const canManage = useCanManageFiles();
  const canRemove = canManage && !!onRemove;
  const web = import.meta.env.VITE_TARGET === 'web';
  const groups = useMemo(() => groupDuplicates(models), [models]);
  // Choices by group hash; a choice whose model is gone (removed, or no longer in the group) falls back to the oldest.
  const [choice, setChoice] = useState<Record<string, string>>({});
  const keepOf = (group: (typeof groups)[number]) => group.members.find(m => m.id === choice[group.hash])?.id ?? group.members[0].id;
  const dropped = groups.flatMap(group => group.members.filter(m => m.id !== keepOf(group)).map(m => m.id));

  const [confirming, setConfirming] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [outcome, setOutcome] = useState<{ removed: number; left: number } | null>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const [focusRequest, setFocusRequest] = useState(0);
  // The group that had the focus may be gone after removing; the heading is always there.
  useEffect(() => { if (focusRequest > 0) headingRef.current?.focus(); }, [focusRequest]);

  const keepAll = (which: 'oldest' | 'newest') => setChoice(Object.fromEntries(groups.map(group => [group.hash, which === 'oldest' ? group.members[0].id : group.members[group.members.length - 1].id])));

  const remove = async () => {
    if (!onRemove || removing) return;
    setRemoving(true);
    setError(null);
    setOutcome(null);
    const ids = [...dropped];
    try {
      const left = (await onRemove(ids)).filter(id => ids.includes(id));
      setOutcome({ removed: ids.length - left.length, left: left.length });
      setConfirming(false);
      setFocusRequest(n => n + 1);
    } catch (e) {
      setError(`${t('dvFailed')} ${formatError(toAppError(e))}`);
    } finally {
      setRemoving(false);
    }
  };

  return (
    <div data-duplicates-view>
      <h2 ref={headingRef} tabIndex={-1} className="m-0 mb-1.5 ui-label text-[var(--ink-3)]">{t('toolDuplicates')}</h2>
      <p className="m-0 mb-3 max-w-[72ch] text-small text-[var(--ink-2)]">{t('dvIntro')}{!web && ` ${t('dvIntroCleanup')}`}</p>
      {outcome && (
        <div role="status" className="mb-3 text-small">
          {outcome.removed > 0 && <p className="m-0 font-medium">{formatCount(t('trashFolderDone'), outcome.removed)}</p>}
          {outcome.left > 0 && <p className="m-0 text-[var(--crit)]">{formatCount(t('dvPartial'), outcome.left)}</p>}
        </div>
      )}
      {groups.length === 0 ? (
        <p className="m-0 py-6 text-center text-[var(--ink-3)]">{t('dvEmpty')}</p>
      ) : (
        <>
          <div className="flex flex-wrap gap-2 mb-3">
            <button type="button" className={buttonClass} onClick={() => keepAll('oldest')}>{t('dvKeepOldest')}</button>
            <button type="button" className={buttonClass} onClick={() => keepAll('newest')}>{t('dvKeepNewest')}</button>
          </div>
          {groups.map((group, index) => {
            const keep = keepOf(group);
            const label = t('dvGroup').replace('{n}', String(index + 1));
            return (
              <section key={group.hash} aria-label={label} className="mb-3 rounded-lg border border-[var(--line)] bg-[var(--panel)] overflow-hidden">
                <header className="flex flex-wrap items-center gap-2 px-3 py-2 border-b border-[var(--line)] bg-[var(--panel-2)]">
                  <h3 className="m-0 text-body font-semibold">{label}</h3>
                  <span className={`${pillClass} border-[var(--line-strong)] text-[var(--ink-2)]`}>{formatCount(t('dvCount'), group.members.length)}</span>
                </header>
                <div role="radiogroup" aria-label={label}>
                  {group.members.map(member => {
                    const kept = member.id === keep;
                    const name = displayName(member.name);
                    const path = displayName(member.path);
                    return (
                      <label key={member.id} className={`flex items-start gap-3 px-3 py-2.5 border-t first:border-t-0 border-[var(--line)] cursor-pointer has-[:focus-visible]:outline has-[:focus-visible]:outline-2 has-[:focus-visible]:-outline-offset-2 has-[:focus-visible]:outline-[var(--accent)] ${kept ? 'bg-[var(--accent-soft)]' : ''}`}>
                        <input type="radio" name={`keep-${group.hash}`} checked={kept} className="mt-1"
                          aria-label={t('dvKeepRadio').replace('{name}', `${name} (${path})`)}
                          onChange={() => setChoice(previous => ({ ...previous, [group.hash]: member.id }))} />
                        <span className="min-w-0 flex-1">
                          <span className="block font-semibold break-words">{name}</span>
                          <span className="block font-code text-caption text-[var(--ink-3)] break-all">{path}</span>
                          <span className="block text-caption text-[var(--ink-2)] tabular-nums">
                            {formatBytes(member.fileSizeBytes, language)} · {t('dvAdded').replace('{date}', formatDate(member.importedAt, language))} · {member.tags.length > 0 ? formatCount(t('dvTags'), member.tags.length) : t('dvNoTags')}
                          </span>
                          <span className="flex flex-wrap gap-1.5 mt-1">
                            <span className={`${pillClass} ${kept ? 'border-[var(--accent)] text-[var(--accent)]' : 'border-[var(--warn)] text-[var(--warn)]'}`}>{t(kept ? 'dvKeepLabel' : 'dvDropLabel')}</span>
                            {member.printStatus === 'printed' && <span className={`${pillClass} border-[var(--line-strong)] text-[var(--ink-2)]`}>{t('dvPrinted')}</span>}
                          </span>
                        </span>
                      </label>
                    );
                  })}
                </div>
              </section>
            );
          })}
          <div className="flex flex-wrap items-center gap-3 mt-3">
            <span aria-live="polite" className="font-medium tabular-nums text-small text-[var(--ink-2)]">
              {formatCount(t('dvSummaryGroups'), groups.length)}, {formatCount(t('dvSummaryDropped'), dropped.length)}
            </span>
            {canRemove ? (
              <button type="button" onClick={() => { setError(null); setConfirming(true); }} disabled={dropped.length === 0} {...lockProps}
                className={`${buttonClass} !border-[var(--accent)] !bg-[var(--accent)] !text-[var(--accent-ink)]`}>
                {t('dvApply')}
              </button>
            ) : <span className="text-small text-[var(--ink-3)]">{t('webReadOnly')}</span>}
          </div>
        </>
      )}
      {confirming && (
        <DuplicateRemoveDialog count={dropped.length} busy={removing} error={error} onConfirm={() => void remove()} onCancel={() => setConfirming(false)} />
      )}
    </div>
  );
}
