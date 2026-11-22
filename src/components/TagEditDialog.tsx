import { useEffect, useState, type RefObject } from 'react';
import type { TagCount } from '../types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { canonicalTag, tagLabel } from '../lib/autoTags';
import { listAllFileTags } from '../lib/api/files';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { useCanEditMetadata } from '../lib/webSession';
import { useImportLock } from '../hooks/ImportLockContext';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';
import { Select } from './Select';

export function TagEditDialog({ source, mode, tags, onSubmit, onDone, onClose, returnFocus }: {
  source: string; mode: 'rename' | 'merge'; tags: TagCount[];
  onSubmit: (source: string, target: string) => Promise<number>;
  onDone: (target: string) => void; onClose: () => void;
  returnFocus: RefObject<HTMLElement | null>;
}) {
  const t = useT();
  const { language } = useLanguage();
  const canEdit = useCanEditMetadata();
  const { jobActive } = useImportLock();
  const [draft, setDraft] = useState(mode === 'rename' ? source : '');
  const [count, setCount] = useState<number | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  // Validation hints only appear once the user has changed something.
  const [touched, setTouched] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  useEffect(() => {
    if (mode !== 'merge') return;
    let cancelled = false;
    listAllFileTags().then(assignments => {
      if (!cancelled) setCount(Object.values(assignments).filter(labels => labels.includes(source)).length);
    }).catch(e => { if (!cancelled) setError(toAppError(e)); });
    return () => { cancelled = true; };
  }, [mode, source]);
  // Custom names retain case and whitespace, matching add_tag; only aliases normalize.
  const target = mode === 'rename' ? canonicalTag(draft) : draft;
  const problem = !draft.trim() ? t('errTagNameEmpty') : target === source ? t('errTagNameUnchanged')
    : mode === 'rename' && tags.some(tag => tag.label === target) ? t('errTagNameExists') : null;
  const blocked = busy || jobActive || !canEdit || !!problem || (mode === 'merge' && count === null);
  const submit = async () => {
    if (blocked) return;
    if (mode === 'merge' && !confirmed) { setConfirmed(true); return; }
    setBusy(true); setError(null);
    try { await onSubmit(source, target); onDone(target); }
    catch (e) { setError(toAppError(e)); }
    finally { setBusy(false); }
  };
  return <CatalogActionDialog title={t(mode === 'rename' ? 'tagMenuRename' : 'tagMenuMerge')}
    busy={busy} returnFocus={returnFocus} onClose={onClose}>
    <form className="space-y-4" onSubmit={event => { event.preventDefault(); void submit(); }}>
      {mode === 'rename' ? <label className="block">{t('tagRenameName')}
        <input data-initial-focus maxLength={100} value={draft} disabled={busy || jobActive || !canEdit}
          onChange={event => { setDraft(event.target.value); setTouched(true); setError(null); }}
          className="block w-full border border-[var(--line)] bg-[var(--panel)] p-2" />
      </label> : <>
        <Select data-initial-focus aria-label={t('tagMergeTarget')} value={draft} disabled={busy || jobActive || !canEdit}
          onValueChange={value => { setDraft(value); setTouched(true); setConfirmed(false); setError(null); }}
          className="block w-full border border-[var(--line)] p-2">
          <option value="" disabled>{t('tagMergeTarget')}</option>
          {tags.filter(tag => tag.label !== source).map(tag => <option key={tag.label} value={tag.label}>{tagLabel(tag.label, language)}</option>)}
        </Select>
        {confirmed && <p>{t('tagMergeConfirm').replace('{count}', String(count)).replace('{source}', tagLabel(source, language)).replace('{target}', tagLabel(target, language))}</p>}
      </>}
      {touched && problem && <p role="alert">{problem}</p>}
      {error && <p role="alert"><ErrorText error={error} /></p>}
      <div className="flex justify-end gap-2">
        <button type="button" className={catalogActionButton} disabled={busy} onClick={onClose}>{t('cancel')}</button>
        <button type="submit" className={catalogActionButton} disabled={blocked}>{t(mode === 'rename' ? 'tagRenameSubmit' : 'tagMergeSubmit')}</button>
      </div>
    </form>
  </CatalogActionDialog>;
}
