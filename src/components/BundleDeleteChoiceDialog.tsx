import { useState, type RefObject } from 'react';
import { displayName } from '../lib/displayName';
import { useImportLock } from '../hooks/ImportLockContext';
import { useT, useFormatCount } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { CatalogActionDialog, catalogActionBase, catalogActionButton } from './CatalogActionDialog';

interface Props {
  /** Names of the bundles that the selection is made of. */
  bundleNames: string[];
  /** Names of every selected file; the trash option lists them. */
  fileNames: string[];
  onClose: () => void;
  returnFocus: RefObject<HTMLElement | null>;
  onDissolve: () => Promise<void>;
  onTrash: () => Promise<void>;
}

/**
 * Asked whenever "Delete" meets a bundle card: deleting the grouping is the safe default, and
 * only the second choice moves files to the trash.
 */
export function BundleDeleteChoiceDialog({ bundleNames, fileNames, onClose, returnFocus, onDissolve, onTrash }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  const { lockProps } = useImportLock();
  const [choice, setChoice] = useState<'dissolve' | 'trash'>('dissolve');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const count = fileNames.length;
  const title = bundleNames.length === 1
    ? t('bundleDeleteTitle').replace('{name}', displayName(bundleNames[0]))
    : t('bundleDeleteTitleMany').replace('{count}', String(bundleNames.length));
  const trashLabel = formatCount(t('bundleDeleteTrash'), count);
  const confirm = async () => {
    setBusy(true);
    setError(null);
    try {
      await (choice === 'dissolve' ? onDissolve() : onTrash());
      onClose();
    } catch (e) {
      setError(toAppError(e));
      setBusy(false);
    }
  };
  return (
    <CatalogActionDialog title={title} busy={busy} onClose={onClose} returnFocus={returnFocus}>
      <p>{formatCount(t('bundleDeleteIntro'), count)}</p>
      <fieldset className="space-y-2" disabled={busy}>
        <legend className="sr-only">{title}</legend>
        <label className="flex items-start gap-2 cursor-pointer">
          <input type="radio" name="bundle-delete" data-initial-focus checked={choice === 'dissolve'} onChange={() => setChoice('dissolve')} className="mt-1" />
          <span>
            <span className="font-semibold">{formatCount(t('bundleDeleteDissolve'), bundleNames.length)}</span>
            <span className="block text-small text-[var(--ink-2)]">{t('bundleDeleteDissolveHint')}</span>
          </span>
        </label>
        <label className="flex items-start gap-2 cursor-pointer">
          <input type="radio" name="bundle-delete" checked={choice === 'trash'} onChange={() => setChoice('trash')} className="mt-1" />
          <span>
            <span className="font-semibold">{trashLabel}</span>
            <span className="block text-small text-[var(--ink-2)]">{t('bundleDeleteTrashHint')}</span>
            <ul className="mt-1.5 pl-5 list-disc text-small max-h-40 overflow-auto">
              {fileNames.map((name, index) => <li key={index} className="break-words">{displayName(name)}</li>)}
            </ul>
          </span>
        </label>
      </fieldset>
      {error && <div role="alert" className="text-[var(--crit)]"><ErrorText error={error} /></div>}
      <div className="flex gap-2 justify-end">
        <button type="button" className={catalogActionButton} disabled={busy} onClick={onClose}>{t('cancel')}</button>
        <button type="button" disabled={busy} {...(choice === 'trash' ? lockProps : {})} onClick={() => void confirm()}
          className={choice === 'trash'
            ? `${catalogActionBase} border-red-500 bg-red-600 text-white hover:bg-red-700`
            : `${catalogActionBase} border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)]`}>
          {choice === 'dissolve' ? t('bundleDissolve') : trashLabel}
        </button>
      </div>
    </CatalogActionDialog>
  );
}
