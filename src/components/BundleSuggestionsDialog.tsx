import { useId, useLayoutEffect, useMemo, useRef, useState, type RefObject } from 'react';
import { displayName } from '../lib/displayName';
import { splitFileName } from '../lib/fileName';
import { formatBytes } from '../i18n/format';
import { useFormatCount, useLanguage, useT } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { MAX_BUNDLE_NAME_CHARS } from '../lib/bundleEdit';
import type { BundleSuggestion } from '../lib/bundleSuggestions';
import type { BundleKind, Folder, ModelFile } from '../types';
import { CatalogActionDialog, catalogActionBase, catalogActionButton } from './CatalogActionDialog';

interface Props {
  open: BundleSuggestion[];
  dismissed: BundleSuggestion[];
  models: ModelFile[];
  folders: Folder[];
  hint: boolean;
  onHintChange: (on: boolean) => void;
  onDismiss: (key: string) => void;
  onRestore: (key: string) => void;
  /** Resolves once the bundles are stored; a rejection is shown in the dialog. */
  onBundle: (items: { name: string; kind: BundleKind; fileIds: string[] }[]) => Promise<number>;
  onClose: () => void;
  returnFocus: RefObject<HTMLElement | null>;
}

interface Draft {
  on: boolean;
  name: string;
  /** Empty until chosen, for a similar name. */
  kind: BundleKind | '';
}

const FIELD = 'h-8 px-2 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] text-small outline-none focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]';
const PILL = 'inline-flex items-center rounded-full border border-[var(--line-strong)] px-2 py-0.5 text-caption';

/**
 * Lists groups of files that probably belong together. Nothing is bundled until the user ticks
 * a suggestion, checks name and kind and confirms; "Do not suggest" hides a group for good.
 */
export function BundleSuggestionsDialog({ open, dismissed, models, folders, hint, onHintChange, onDismiss, onRestore, onBundle, onClose, returnFocus }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  const { language } = useLanguage();
  const byId = useMemo(() => new Map(models.map(model => [model.id, model])), [models]);
  const [drafts, setDrafts] = useState<Record<string, Draft>>({});
  const [showDismissed, setShowDismissed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const [live, setLive] = useState('');
  const pendingFocus = useRef<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const idBase = useId();

  const draftOf = (suggestion: BundleSuggestion): Draft => drafts[suggestion.key]
    ?? { on: suggestion.reason === 'sameModelId', name: suggestion.name, kind: suggestion.kind ?? '' };
  const change = (suggestion: BundleSuggestion, patch: Partial<Draft>) =>
    setDrafts(previous => ({ ...previous, [suggestion.key]: { ...draftOf(suggestion), ...patch } }));

  // Focus goes to a stable control after a list change, so it is not lost with the row that vanished.
  useLayoutEffect(() => {
    const selector = pendingFocus.current;
    if (!selector) return;
    pendingFocus.current = null;
    const target = rootRef.current?.querySelector<HTMLElement>(selector) ?? rootRef.current?.querySelector<HTMLElement>('[data-close]');
    target?.focus({ preventScroll: true });
  });

  const chosen = open.filter(suggestion => draftOf(suggestion).on);
  const incomplete = (suggestion: BundleSuggestion) => {
    const draft = draftOf(suggestion);
    return draft.kind === '' || draft.name.trim() === '';
  };
  const canSubmit = !busy && chosen.length > 0 && !chosen.some(incomplete);

  const folderName = (id: string) => folders.find(folder => folder.id === id)?.name ?? '';
  const originOf = (suggestion: BundleSuggestion) => 'folderId' in suggestion.origin
    ? t('bundleSuggestOriginFolder').replace('{name}', displayName(folderName(suggestion.origin.folderId)))
    : t('bundleSuggestOriginModel').replace('{platform}', suggestion.origin.platform).replace('{id}', suggestion.origin.modelId);
  const memberMeta = (model: ModelFile) => [
    splitFileName(model.name).extension.replace('.', '').toUpperCase(),
    formatBytes(model.fileSizeBytes, language),
  ].filter(Boolean).join(' · ');

  const submit = async () => {
    if (!canSubmit) return;
    setBusy(true);
    setError(null);
    try {
      const made = await onBundle(chosen.map(suggestion => {
        const draft = draftOf(suggestion);
        return { name: draft.name.trim(), kind: draft.kind as BundleKind, fileIds: suggestion.fileIds };
      }));
      pendingFocus.current = '[data-close]';
      const models = chosen.slice(0, made).reduce((sum, suggestion) => sum + suggestion.fileIds.length, 0);
      setLive(made > 0 ? formatCount(t('bundleSuggestToastCreated'), made).replace('{models}', String(models)) : '');
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setBusy(false);
    }
  };

  const dismiss = (suggestion: BundleSuggestion) => {
    pendingFocus.current = '[data-hidden-toggle]';
    setLive(t('bundleSuggestLiveDismissed').replace('{name}', displayName(draftOf(suggestion).name)));
    onDismiss(suggestion.key);
  };
  const restore = (suggestion: BundleSuggestion) => {
    pendingFocus.current = dismissed.length > 1 ? '[data-hidden-toggle]' : '[data-close]';
    setLive(t('bundleSuggestLiveRestored').replace('{name}', displayName(draftOf(suggestion).name)));
    onRestore(suggestion.key);
  };

  const row = (suggestion: BundleSuggestion, isDismissed: boolean, first = false) => {
    const draft = draftOf(suggestion);
    const fieldId = `${idBase}-${suggestion.key}`;
    const members = suggestion.fileIds.flatMap(id => byId.get(id) ?? []);
    const kindMissing = !isDismissed && draft.on && draft.kind === '';
    return (
      <li key={suggestion.key} data-suggestion={suggestion.key}
        className="flex gap-2 rounded-[4px] border border-[var(--line)] bg-[var(--panel)] px-2 py-2">
        {!isDismissed && (
          <input type="checkbox" className="mt-1 self-start" checked={draft.on} disabled={busy} data-initial-focus={first ? '' : undefined}
            aria-label={t('bundleSuggestCheck').replace('{name}', displayName(draft.name))}
            onChange={event => change(suggestion, { on: event.target.checked })} />
        )}
        <div className="min-w-0 flex-1 space-y-2">
          <div className="flex flex-wrap items-center gap-1.5">
            <span className={`${PILL} ${suggestion.reason === 'sameModelId' ? 'text-[var(--accent)] border-[var(--accent)]' : ''}`}>
              {t(suggestion.reason === 'sameModelId' ? 'bundleSuggestReasonModel' : 'bundleSuggestReasonName')}
            </span>
            <span className="text-caption text-[var(--ink-3)]">{originOf(suggestion)}</span>
          </div>
          <ul className="space-y-0.5">
            {members.map(model => (
              <li key={model.id} className="flex items-baseline gap-2 min-w-0">
                <span className="truncate text-small" title={displayName(model.name)}>{displayName(model.name)}</span>
                <span className="flex-none text-caption text-[var(--ink-3)]">{memberMeta(model)}</span>
              </li>
            ))}
          </ul>
          {isDismissed ? (
            <button type="button" className={catalogActionButton} disabled={busy}
              aria-label={t('bundleSuggestRestoreAria').replace('{name}', displayName(draft.name))}
              onClick={() => restore(suggestion)}>{t('bundleSuggestRestore')}</button>
          ) : (
            <div className="flex flex-wrap items-end gap-2">
              <div className="space-y-0.5 flex-[2_1_12rem]">
                <label htmlFor={`${fieldId}-name`} className="block text-caption font-semibold">{t('bundleSuggestName')}</label>
                <input id={`${fieldId}-name`} value={draft.name} maxLength={MAX_BUNDLE_NAME_CHARS} disabled={busy}
                  aria-invalid={draft.on && draft.name.trim() === ''}
                  onChange={event => change(suggestion, { name: event.target.value })} className={`${FIELD} w-full`} />
              </div>
              <div className="space-y-0.5 flex-[1_1_9rem]">
                <label htmlFor={`${fieldId}-kind`} className="block text-caption font-semibold">{t('bundleSuggestKind')}</label>
                <select id={`${fieldId}-kind`} value={draft.kind} disabled={busy} aria-invalid={kindMissing}
                  onChange={event => change(suggestion, { kind: event.target.value as BundleKind | '' })} className={`${FIELD} w-full`}>
                  <option value="">{t('bundleSuggestKindChoose')}</option>
                  <option value="variants">{t('bundleKindVariants')}</option>
                  <option value="parts">{t('bundleDlgKindParts')}</option>
                </select>
              </div>
              <button type="button" className={catalogActionButton} disabled={busy}
                aria-label={t('bundleSuggestDismissAria').replace('{name}', displayName(draft.name))}
                onClick={() => dismiss(suggestion)}>{t('bundleSuggestDismiss')}</button>
            </div>
          )}
          {kindMissing && <div role="alert" className="text-caption text-[var(--crit)]">{t('bundleSuggestKindMissing')}</div>}
        </div>
      </li>
    );
  };

  return (
    <CatalogActionDialog title={t('bundleSuggestTitle')} wide busy={busy} onClose={onClose} returnFocus={returnFocus}>
      <div ref={rootRef} className="space-y-4">
        <p className="text-[var(--ink-2)]">{t('bundleSuggestIntro')}</p>

        {open.length === 0
          ? <p className="py-3 text-[var(--ink-2)]">{t('bundleSuggestNone')}</p>
          : <ul aria-label={t('bundleSuggestListAria')} className="space-y-2">{open.map((suggestion, index) => row(suggestion, false, index === 0))}</ul>}

        {dismissed.length > 0 && (
          <div className="space-y-1.5">
            <button type="button" data-hidden-toggle aria-expanded={showDismissed} aria-controls={`${idBase}-dismissed`}
              onClick={() => setShowDismissed(shown => !shown)}
              className="text-small underline underline-offset-2 cursor-pointer text-[var(--ink-2)] hover:text-[var(--ink)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
              {t('bundleSuggestDismissedHeading').replace('{count}', String(dismissed.length))}
            </button>
            <ul id={`${idBase}-dismissed`} hidden={!showDismissed} className="space-y-2">
              {showDismissed && dismissed.map(suggestion => row(suggestion, true))}
            </ul>
          </div>
        )}

        <label className="flex items-center gap-2 cursor-pointer">
          <input type="checkbox" checked={hint} onChange={event => onHintChange(event.target.checked)} />
          {t('bundleSuggestHintSwitch')}
        </label>

        <div role="status" aria-live="polite" className="text-caption text-[var(--ink-2)] min-h-[1.2em]">{live}</div>
        {error && <div role="alert" className="text-[var(--crit)]"><ErrorText error={error} /></div>}

        <div className="flex gap-2 justify-end">
          <button type="button" data-close data-initial-focus={open.length === 0 ? '' : undefined} className={catalogActionButton} disabled={busy} onClick={onClose}>{t('bundleSuggestClose')}</button>
          <button type="button" className={`${catalogActionBase} border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)]`}
            disabled={!canSubmit} onClick={() => void submit()}>
            {t('bundleSuggestSubmit').replace('{count}', String(chosen.length))}
          </button>
        </div>
      </div>
    </CatalogActionDialog>
  );
}
