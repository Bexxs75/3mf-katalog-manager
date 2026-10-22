import { useEffect, useMemo, useState, type RefObject } from 'react';
import * as filesApi from '../lib/api/files';
import type { DateRenamePreview, DateRenameSource, DateRenameStatus } from '../lib/api/files';
import { messageOf } from '../lib/errors';
import { useFormatCount, useT } from '../i18n/LanguageContext';
import type { Translations } from '../i18n/types';
import { useImportLock } from '../hooks/ImportLockContext';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';

const SOURCES: { value: DateRenameSource; label: 'dateRenameSourceModified' | 'dateRenameSourceImported' }[] = [
  { value: 'modified', label: 'dateRenameSourceModified' },
  { value: 'imported', label: 'dateRenameSourceImported' },
];

const SKIP_REASONS: Record<Exclude<DateRenameStatus, 'rename'>, keyof Translations> = {
  skip_has_prefix: 'dateRenameSkipHasPrefix',
  skip_conflict: 'dateRenameSkipConflict',
  skip_missing: 'dateRenameSkipMissing',
  skip_trash: 'dateRenameSkipTrash',
  skip_too_long: 'dateRenameSkipTooLong',
  skip_external: 'dateRenameSkipExternal',
  skip_no_date: 'dateRenameSkipNoDate',
  skip_invalid: 'dateRenameSkipInvalid',
};

export interface DateRenameOutcome { renamed: number; failed: number }

interface Props {
  fileIds: string[];
  onClose: () => void;
  onDone: (outcome: DateRenameOutcome) => void;
  returnFocus?: RefObject<HTMLElement | null>;
}

/** Preview and confirmation for prefixing existing files with a date; nothing is renamed before the confirm button. */
export function DateRenameDialog({ fileIds, onClose, onDone, returnFocus }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  const { jobActive, lockProps } = useImportLock();
  const [source, setSource] = useState<DateRenameSource>('modified');
  const [preview, setPreview] = useState<DateRenamePreview[]>([]);
  const [loading, setLoading] = useState(true);
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const tooMany = fileIds.length > filesApi.DATE_RENAME_MAX_ITEMS;

  useEffect(() => {
    if (tooMany) { setLoading(false); return; }
    // A slower answer for the previous date source must not overwrite a newer one.
    let current = true;
    setLoading(true);
    setError(null);
    filesApi.previewDateRename(fileIds, source).then(
      rows => {
        if (!current) return;
        setPreview(rows);
        setChecked(new Set(rows.filter(row => row.status === 'rename').map(row => row.id)));
        setLoading(false);
      },
      e => {
        if (!current) return;
        setPreview([]);
        setChecked(new Set());
        setError(t('dateRenamePreviewFailed').replace('{error}', () => messageOf(e)));
        setLoading(false);
      },
    );
    return () => { current = false; };
  // `t` changes with the language only; the preview must not reload for that.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fileIds, source, tooMany]);

  const selected = useMemo(() => preview.filter(row => row.status === 'rename' && checked.has(row.id)), [preview, checked]);

  const toggle = (id: string) => setChecked(previous => {
    const next = new Set(previous);
    if (next.has(id)) next.delete(id); else next.add(id);
    return next;
  });

  const apply = async () => {
    if (selected.length === 0 || applying || loading) return;
    setApplying(true);
    setError(null);
    try {
      // Only id and the previewed name leave the dialog; the backend re-checks both.
      const results = await filesApi.applyDateRename(selected.map(row => ({ id: row.id, newName: row.newName })));
      const renamed = results.filter(result => result.status === 'renamed').length;
      onDone({ renamed, failed: results.length - renamed });
    } catch (e) {
      setError(t('dateRenameApplyFailed').replace('{error}', () => messageOf(e)));
      setApplying(false);
    }
  };

  return (
    <CatalogActionDialog title={t('dateRenameTitle')} onClose={onClose} returnFocus={returnFocus} busy={applying} wide>
      <div role="radiogroup" aria-label={t('dateRenameSourceLabel')} className="flex items-center gap-2">
        <span className="text-small text-[var(--ink-2)]">{t('dateRenameSourceLabel')}:</span>
        <div className="inline-flex rounded-[3px] border border-[var(--line-strong)] overflow-hidden">
          {SOURCES.map((option, index) => (
            <label key={option.value}
              className={`px-3 h-8 inline-flex items-center text-small font-semibold cursor-pointer focus-within:outline focus-within:outline-2 focus-within:-outline-offset-2 focus-within:outline-[var(--accent)] ${source === option.value ? 'bg-[var(--accent)] text-[var(--accent-ink)]' : 'bg-[var(--panel)] text-[var(--ink-2)] hover:text-[var(--ink)]'}`}>
              <input type="radio" name="date-rename-source" className="sr-only" value={option.value}
                {...(index === 0 ? { 'data-initial-focus': '' } : {})}
                checked={source === option.value} disabled={applying}
                onChange={() => setSource(option.value)} />
              {t(option.label)}
            </label>
          ))}
        </div>
      </div>
      <p className="text-small text-[var(--ink-2)]">
        {t('dateRenameFormat')} · {formatCount(t('dateRenameSelectedModels'), fileIds.length)}
      </p>

      {tooMany ? (
        <p role="alert" className="text-small text-red-400">{t('dateRenameTooMany').replace('{max}', String(filesApi.DATE_RENAME_MAX_ITEMS))}</p>
      ) : (
        <div aria-busy={loading} className="max-h-[45vh] overflow-auto border border-[var(--line)] rounded-[3px]">
          <p aria-live="polite" className={loading ? 'px-3 py-2 text-small text-[var(--ink-3)]' : 'sr-only'}>
            {loading ? t('dateRenameLoading') : ''}
          </p>
          {!loading && preview.length > 0 && (
            <table className="w-full text-small">
              <caption className="sr-only">{t('dateRenameTableCaption')}</caption>
              <tbody>
                {preview.map(row => {
                  const skipped = row.status !== 'rename';
                  const conflict = row.status === 'skip_conflict';
                  const reason = skipped ? t(SKIP_REASONS[row.status as Exclude<DateRenameStatus, 'rename'>]) as string : '';
                  const label = skipped
                    ? t('dateRenameRowSkippedAria').replace('{old}', () => row.oldName || row.id).replace('{reason}', () => reason)
                    : t('dateRenameRowAria').replace('{old}', () => row.oldName).replace('{new}', () => row.newName);
                  return (
                    <tr key={row.id} className="border-b border-[var(--line)] last:border-b-0 align-top">
                      <td className="w-8 px-2 py-1.5">
                        <input type="checkbox" aria-label={label} disabled={skipped || applying}
                          checked={!skipped && checked.has(row.id)} onChange={() => toggle(row.id)} />
                      </td>
                      <td className={`px-2 py-1.5 break-all ${conflict ? 'text-red-400' : 'text-[var(--ink)]'}`}>
                        {skipped ? (
                          <>
                            <span className={conflict ? '' : 'text-[var(--ink-3)]'}>{row.oldName || row.id}</span>
                            {' '}<span className={conflict ? 'text-red-400' : 'text-[var(--ink-3)]'}>({reason})</span>
                          </>
                        ) : (
                          <>
                            <span className="text-[var(--ink-3)] line-through">{row.oldName}</span>
                            <span aria-hidden="true" className="px-1.5 text-[var(--ink-3)]">→</span>
                            <span>{row.newName}</span>
                          </>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </div>
      )}

      {error && <p role="alert" className="text-small text-red-400 break-words">{error}</p>}
      <p className="text-caption text-[var(--ink-3)]">{t('dateRenameHint')}</p>

      <div className="flex justify-end gap-2">
        <button type="button" className={catalogActionButton} disabled={applying} onClick={onClose}>{t('cancel')}</button>
        <button type="button" {...lockProps}
          className="h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-small font-semibold cursor-pointer hover:bg-[var(--panel-2)] hover:text-[var(--ink)] disabled:opacity-50 disabled:cursor-default"
          disabled={selected.length === 0 || loading || applying || tooMany || jobActive}
          onClick={() => void apply()}>
          {applying ? t('dateRenameApplying') : formatCount(t('dateRenameApply'), selected.length)}
        </button>
      </div>
    </CatalogActionDialog>
  );
}
