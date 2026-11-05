import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type RefObject } from 'react';
import { displayName } from '../lib/displayName';
import { splitFileName } from '../lib/fileName';
import { formatBytes } from '../i18n/format';
import { useLanguage, useT, useFormatCount } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { MAX_BUNDLE_MEMBERS, MIN_BUNDLE_MEMBERS } from '../lib/bundles';
import { MAX_BUNDLE_LABEL_CHARS, MAX_BUNDLE_NAME_CHARS, bundleRoom, duplicateIds, switchNotes, type EditRow } from '../lib/bundleEdit';
import type { Bundle, BundleKind, ModelFile } from '../types';
import { CatalogActionDialog, catalogActionBase, catalogActionButton } from './CatalogActionDialog';
import { Icon } from './Icon';

export type BundleDialogRequest =
  | { mode: 'create'; fileIds: string[] }
  | { mode: 'add'; fileIds: string[]; targetId: string | null }
  | { mode: 'edit'; bundleId: string };

interface Props {
  request: BundleDialogRequest;
  models: ModelFile[];
  bundles: Bundle[];
  onClose: () => void;
  returnFocus: RefObject<HTMLElement | null>;
  /** Each save resolves once the change is stored; a rejection is shown in the dialog. */
  onCreate: (name: string, kind: BundleKind, rows: { fileId: string; label: string }[]) => Promise<void>;
  onAdd: (bundleId: string, fileIds: string[]) => Promise<void>;
  onSaveEdit: (bundle: Bundle, draft: { name: string; kind: BundleKind; rows: EditRow[] }) => Promise<void>;
}

const FIELD = 'h-8 px-2 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] text-small outline-none focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]';
const ICON_BUTTON = 'h-8 w-8 grid place-items-center rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] cursor-pointer hover:border-[var(--accent)] disabled:opacity-40 disabled:cursor-default focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]';
const NOTICE = 'rounded-[4px] border border-[var(--line-strong)] bg-[var(--panel-2)] px-3 py-2 text-small space-y-1';

/** Moves one entry of a list to another place; the list stays as it is for places out of range. */
function moved<T>(list: T[], from: number, to: number): T[] {
  if (to < 0 || to >= list.length || from === to) return list;
  const next = [...list];
  next.splice(to, 0, next.splice(from, 1)[0]);
  return next;
}

/**
 * One dialog for the three ways to change a bundle: create one from the selection, add the
 * selection to an existing one, and edit name, kind, order and labels of an existing one.
 * The order can be changed without a mouse (arrow buttons, or arrow keys on the grip).
 */
export function BundleDialog({ request, models, bundles, onClose, returnFocus, onCreate, onAdd, onSaveEdit }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  const { language } = useLanguage();
  const byId = useMemo(() => new Map(models.map(model => [model.id, model])), [models]);
  const nameOf = (id: string) => displayName(byId.get(id)?.name ?? id);
  const edited = request.mode === 'edit' ? bundles.find(bundle => bundle.id === request.bundleId) ?? null : null;

  const initialIds = useMemo(() => {
    if (request.mode === 'edit') {
      return (edited?.members ?? []).filter(member => !member.inTrash && byId.has(member.fileId)).map(member => member.fileId);
    }
    return request.fileIds.filter(id => byId.has(id));
    // The rows are a draft: they start from the request once and are not rebuilt when the catalog reloads.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const [rows, setRows] = useState<EditRow[]>(() => initialIds.map(fileId => ({
    fileId,
    label: edited?.members.find(member => member.fileId === fileId)?.label ?? '',
    keep: true,
  })));
  const [name, setName] = useState(() => edited?.name ?? splitFileName(byId.get(initialIds[0])?.name ?? '').base);
  const [kind, setKind] = useState<BundleKind>(edited?.kind ?? 'variants');
  const [targetId, setTargetId] = useState(() => request.mode === 'add' ? request.targetId ?? bundles[0]?.id ?? '' : '');
  const [chosen, setChosen] = useState<Set<string>>(() => new Set(initialIds));
  const [confirmed, setConfirmed] = useState(false);
  const [live, setLive] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const pendingFocus = useRef<string | null>(null);
  const nameId = useId();
  const counterId = useId();
  const errorId = useId();
  const dialogError = useId();

  useLayoutEffect(() => {
    if (!pendingFocus.current) return;
    const target = listRef.current?.querySelector<HTMLElement>(pendingFocus.current);
    pendingFocus.current = null;
    target?.focus({ preventScroll: true });
  }, [rows]);
  // A bundle that vanished while the dialog was open (deleted on another device) cannot be edited any more.
  useEffect(() => { if (request.mode === 'edit' && !edited) onClose(); }, [request.mode, edited, onClose]);

  const duplicates = useMemo(() => duplicateIds(initialIds.flatMap(id => byId.get(id) ?? [])), [initialIds, byId]);
  const target = request.mode === 'add' ? bundles.find(bundle => bundle.id === targetId) ?? null : null;
  const keptRows = rows.filter(row => row.keep);
  const trimmedName = name.trim();
  const nameMissing = request.mode !== 'add' && trimmedName === '';

  // Ids that join the bundle in create/add mode.
  const joining = request.mode === 'add'
    ? initialIds.filter(id => chosen.has(id) && !target?.members.some(member => member.fileId === id))
    : request.mode === 'create' ? rows.map(row => row.fileId) : [];
  const notes = request.mode === 'edit' ? [] : switchNotes(joining, bundles, request.mode === 'add' ? targetId : null);
  const needsConfirmation = notes.length > 0 && !confirmed;
  const tooMany = request.mode === 'add' && target !== null && !bundleRoom(target, joining).fits;
  const dissolving = request.mode === 'edit' && keptRows.length < MIN_BUNDLE_MEMBERS;

  const move = (from: number, to: number, focus: 'grip' | 'up' | 'down') => {
    const last = keptRows.length - 1;
    if (to < 0 || to > last) return;
    const row = rows[from];
    // After the step the button that caused it may be disabled (first or last place): focus its partner then.
    const button = focus === 'up' ? (to > 0 ? 'up' : 'down') : focus === 'down' ? (to < last ? 'down' : 'up') : 'grip';
    pendingFocus.current = `[data-${button}="${row.fileId}"]`;
    setRows(moved(rows, from, to));
    setLive(t(to === 0 ? 'bundleDlgLiveMovedTitle' : 'bundleDlgLiveMoved')
      .replace('{name}', nameOf(row.fileId)).replace('{position}', String(to + 1)).replace('{total}', String(keptRows.length)));
  };
  const makeTitle = (index: number) => {
    const row = rows[index];
    const next = [...rows];
    // The title model and the chosen one swap places, like the command behind the button.
    [next[0], next[index]] = [next[index], next[0]];
    pendingFocus.current = `[data-grip="${row.fileId}"]`;
    setRows(next);
    setLive(t('bundleDlgLiveTitle').replace('{name}', nameOf(row.fileId)));
  };
  const takeOut = (index: number) => {
    const row = rows[index];
    pendingFocus.current = '[data-grip]';
    if (request.mode === 'edit') {
      // Kept at the end of the list until saved, so "keep after all" stays within reach.
      const next = rows.filter((_, i) => i !== index);
      setRows([...next.filter(r => r.keep), ...next.filter(r => !r.keep), { ...row, keep: false }]);
      setLive(t('bundleDlgLiveWillRemove').replace('{name}', nameOf(row.fileId)));
    } else {
      setRows(rows.filter((_, i) => i !== index));
      setLive(t('bundleDlgLiveTakenOut').replace('{name}', nameOf(row.fileId)).replace('{count}', String(rows.length - 1)));
    }
  };
  const keepAfterAll = (index: number) => {
    const row = rows[index];
    const next = rows.filter((_, i) => i !== index);
    pendingFocus.current = `[data-grip="${row.fileId}"]`;
    setRows([...next.filter(r => r.keep), { ...row, keep: true }, ...next.filter(r => !r.keep)]);
    setLive(t('bundleDlgLiveKept').replace('{name}', nameOf(row.fileId)));
  };
  const setLabel = (index: number, label: string) => setRows(rows.map((row, i) => i === index ? { ...row, label } : row));

  const submit = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      if (request.mode === 'create') await onCreate(trimmedName, kind, rows.map(row => ({ fileId: row.fileId, label: row.label.trim() })));
      else if (request.mode === 'add') await onAdd(targetId, joining);
      else if (edited) await onSaveEdit(edited, { name: trimmedName, kind, rows });
      onClose();
    } catch (e) {
      setError(toAppError(e));
      setBusy(false);
    }
  };

  const title = t(request.mode === 'create' ? 'bundleDlgCreateTitle' : request.mode === 'edit' ? 'bundleDlgEditTitle' : 'bundleDlgAddTitle');
  const intro = request.mode === 'create' ? formatCount(t('bundleDlgCreateIntro'), rows.length)
    : request.mode === 'edit' ? t('bundleDlgEditIntro') : t('bundleDlgAddIntro');
  const submitLabel = request.mode === 'create' ? formatCount(t('bundleDlgCreateSubmit'), rows.length)
    : request.mode === 'add' ? formatCount(t('bundleDlgAddSubmit'), joining.length)
    : dissolving ? t('bundleDissolve') : t('bundleDlgSave');
  const submitDisabled = busy || nameMissing || needsConfirmation || tooMany
    || (request.mode === 'create' && rows.length < MIN_BUNDLE_MEMBERS)
    || (request.mode === 'add' && (joining.length === 0 || !target));

  const meta = (id: string) => {
    const model = byId.get(id);
    if (!model) return '';
    const ext = splitFileName(model.name).extension.replace('.', '').toUpperCase();
    return [ext, formatBytes(model.fileSizeBytes, language)].filter(Boolean).join(' · ');
  };

  return (
    <CatalogActionDialog title={title} wide busy={busy} onClose={onClose} returnFocus={returnFocus}>
      <p className="text-[var(--ink-2)]">{intro}</p>

      {request.mode !== 'add' && <>
        <div className="space-y-1">
          <label htmlFor={nameId} className="block font-semibold">{t('bundleDlgName')}</label>
          <input id={nameId} data-initial-focus value={name} maxLength={MAX_BUNDLE_NAME_CHARS} disabled={busy}
            aria-invalid={nameMissing} aria-describedby={`${counterId} ${errorId}`}
            onChange={event => setName(event.target.value)} className={`${FIELD} w-full`} />
          <div id={counterId} className="text-caption text-[var(--ink-3)] tabular-nums">
            {t('bundleDlgNameCount').replace('{count}', String(name.length)).replace('{max}', String(MAX_BUNDLE_NAME_CHARS))}
          </div>
          <div id={errorId} role="alert" className="text-caption text-[var(--crit)]">{nameMissing ? t('bundleDlgNameEmpty') : ''}</div>
        </div>

        <fieldset className="space-y-1.5" disabled={busy}>
          <legend className="font-semibold mb-1">{t('bundleDlgKind')}</legend>
          {(['variants', 'parts'] as const).map(value => (
            <label key={value} className="flex items-start gap-2 cursor-pointer">
              <input type="radio" name="bundle-kind" value={value} checked={kind === value} onChange={() => setKind(value)} className="mt-1" />
              <span>
                <span className="font-semibold">{t(value === 'variants' ? 'bundleKindVariants' : 'bundleDlgKindParts')}</span>
                <span className="block text-small text-[var(--ink-2)]">{t(value === 'variants' ? 'bundleDlgKindVariantsHint' : 'bundleDlgKindPartsHint')}</span>
              </span>
            </label>
          ))}
        </fieldset>

        <div className="space-y-1">
          <div id={`${nameId}-members`} className="font-semibold">
            {t('bundleDlgMembers')} <span className="font-normal text-[var(--ink-3)] text-small">{t('bundleDlgMembersHint')}</span>
          </div>
          <ul ref={listRef} aria-labelledby={`${nameId}-members`} className="space-y-1.5">
            {rows.map((row, index) => {
              const position = index + 1;
              const total = keptRows.length;
              if (!row.keep) {
                return (
                  <li key={row.fileId} className="flex items-center gap-2 rounded-[4px] border border-dashed border-[var(--line-strong)] px-2 py-1.5 opacity-80">
                    <span className="min-w-0 flex-1">
                      <span className="block font-semibold text-small truncate">{nameOf(row.fileId)}</span>
                      <span className="block text-caption text-[var(--ink-2)]">{t('bundleDlgWillRemove')}</span>
                    </span>
                    <button type="button" className={catalogActionButton} onClick={() => keepAfterAll(index)}>{t('bundleDlgKeep')}</button>
                  </li>
                );
              }
              return (
                <li key={row.fileId} data-row={row.fileId}
                  draggable={dragFrom !== null}
                  onDragStart={event => { event.dataTransfer?.setData?.('text/plain', row.fileId); }}
                  onDragOver={event => { if (dragFrom !== null) event.preventDefault(); }}
                  onDrop={event => {
                    event.preventDefault();
                    if (dragFrom === null) return;
                    const from = dragFrom;
                    setDragFrom(null);
                    move(from, index, 'grip');
                  }}
                  onDragEnd={() => setDragFrom(null)}
                  className="flex flex-wrap items-center gap-2 rounded-[4px] border border-[var(--line)] bg-[var(--panel)] px-2 py-1.5">
                  <button type="button" data-grip={row.fileId} disabled={busy}
                    aria-label={t('bundleDlgGripAria').replace('{name}', nameOf(row.fileId)).replace('{position}', String(position)).replace('{total}', String(total))}
                    title={t('bundleDlgGripTitle')}
                    onMouseDown={() => setDragFrom(index)} onMouseUp={() => setDragFrom(null)}
                    onKeyDown={event => {
                      if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return;
                      event.preventDefault();
                      move(index, index + (event.key === 'ArrowUp' ? -1 : 1), 'grip');
                    }}
                    className={`${ICON_BUTTON} cursor-grab`}>
                    <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true" fill="currentColor"><circle cx="4.5" cy="3" r="1.2" /><circle cx="9.5" cy="3" r="1.2" /><circle cx="4.5" cy="7" r="1.2" /><circle cx="9.5" cy="7" r="1.2" /><circle cx="4.5" cy="11" r="1.2" /><circle cx="9.5" cy="11" r="1.2" /></svg>
                  </button>
                  <span aria-hidden="true" className="w-5 text-center tabular-nums text-[var(--ink-3)]">{position}</span>
                  <span className="min-w-[8rem] flex-1">
                    <span className="block font-semibold text-small truncate" title={nameOf(row.fileId)}>{nameOf(row.fileId)}</span>
                    <span className="block text-caption text-[var(--ink-3)]">
                      {meta(row.fileId)}
                      {index === 0 && <> · <span className="font-semibold text-[var(--accent)]">{t('bundleDlgTitleBadge')}</span></>}
                      {duplicates.has(row.fileId) && <> · <span className="font-semibold">{t('bundleDlgDuplicate')}</span></>}
                    </span>
                  </span>
                  <input value={row.label} maxLength={MAX_BUNDLE_LABEL_CHARS} disabled={busy}
                    placeholder={t('bundleDlgLabelPlaceholder')}
                    aria-label={t('bundleDlgLabelAria').replace('{name}', nameOf(row.fileId)).replace('{max}', String(MAX_BUNDLE_LABEL_CHARS))}
                    onChange={event => setLabel(index, event.target.value)} className={`${FIELD} w-52 max-w-full`} />
                  {index > 0 && <button type="button" disabled={busy} title={t('bundleDlgMakeTitleHint')} onClick={() => makeTitle(index)} className={catalogActionButton}>{t('bundleDlgMakeTitle')}</button>}
                  <button type="button" data-up={row.fileId} disabled={busy || index === 0} aria-label={t('bundleDlgMoveUp').replace('{name}', nameOf(row.fileId))}
                    onClick={() => move(index, index - 1, 'up')} className={ICON_BUTTON}><Icon name="arrow-up" size={14} /></button>
                  <button type="button" data-down={row.fileId} disabled={busy || index === total - 1} aria-label={t('bundleDlgMoveDown').replace('{name}', nameOf(row.fileId))}
                    onClick={() => move(index, index + 1, 'down')} className={ICON_BUTTON}><Icon name="arrow-down" size={14} /></button>
                  <button type="button" disabled={busy || (request.mode === 'create' && rows.length <= MIN_BUNDLE_MEMBERS)}
                    title={request.mode === 'create' && rows.length <= MIN_BUNDLE_MEMBERS ? t('bundleDlgNeedTwo') : undefined}
                    onClick={() => takeOut(index)} className={catalogActionButton}>
                    {t(request.mode === 'edit' ? 'bundleDlgRemove' : 'bundleDlgTakeOut')}
                  </button>
                </li>
              );
            })}
          </ul>
          <div role="status" aria-live="polite" className="text-caption text-[var(--ink-2)] min-h-[1.2em]">{live}</div>
        </div>
      </>}

      {request.mode === 'add' && (bundles.length === 0 ? <p>{t('bundleDlgNoBundles')}</p> : <>
        <div className="space-y-1">
          <label htmlFor={nameId} className="block font-semibold">{t('bundleDlgTarget')}</label>
          <select id={nameId} data-initial-focus value={targetId} disabled={busy} onChange={event => setTargetId(event.target.value)} className={`${FIELD} w-full`}>
            {bundles.map(bundle => (
              <option key={bundle.id} value={bundle.id}>
                {t('bundleDlgTargetOption').replace('{name}', displayName(bundle.name))
                  .replace('{kind}', t(bundle.kind === 'variants' ? 'bundleKindVariants' : 'bundleKindParts'))
                  .replace('{count}', String(bundle.members.length))}
              </option>
            ))}
          </select>
        </div>
        <fieldset className="space-y-1">
          <legend className="font-semibold mb-1">{t('bundleDlgModels')}</legend>
          <ul className="space-y-1">
            {initialIds.map(id => {
              const inTarget = target?.members.some(member => member.fileId === id) ?? false;
              return (
                <li key={id}>
                  <label className={`flex items-start gap-2 rounded-[4px] border border-[var(--line)] bg-[var(--panel)] px-2 py-1.5 ${inTarget ? 'opacity-60' : 'cursor-pointer'}`}>
                    <input type="checkbox" className="mt-1" disabled={busy || inTarget} checked={chosen.has(id) && !inTarget}
                      aria-label={t('bundleDlgAddCheck').replace('{name}', nameOf(id))}
                      onChange={event => setChosen(previous => { const next = new Set(previous); if (event.target.checked) next.add(id); else next.delete(id); return next; })} />
                    <span className="min-w-0">
                      <span className="block font-semibold text-small truncate">{nameOf(id)}</span>
                      <span className="block text-caption text-[var(--ink-3)]">
                        {meta(id)}{inTarget && <> · {t('bundleDlgAlreadyIn')}</>}
                        {duplicates.has(id) && <> · <span className="font-semibold">{t('bundleDlgDuplicate')}</span></>}
                      </span>
                    </span>
                  </label>
                </li>
              );
            })}
          </ul>
        </fieldset>
        {target && <div className={NOTICE}>
          {tooMany
            ? t('bundleDlgAddTooMany').replace('{max}', String(MAX_BUNDLE_MEMBERS))
            : t('bundleDlgAddLimit').replace('{name}', displayName(target.name)).replace('{count}', String(bundleRoom(target, joining).total)).replace('{max}', String(MAX_BUNDLE_MEMBERS))}
        </div>}
      </>)}

      {notes.length > 0 && (
        <div className={NOTICE} data-switch-note>
          {notes.map(note => (
            <div key={note.fileId}>
              <b>{t('bundleDlgSwitch').replace('{name}', nameOf(note.fileId)).replace('{from}', displayName(note.from.name)).replace('{to}', displayName(request.mode === 'add' ? target?.name ?? '' : trimmedName || '…'))}</b>{' '}
              <span className="text-[var(--ink-2)]">{note.left >= MIN_BUNDLE_MEMBERS
                ? t('bundleDlgSwitchKeeps').replace('{from}', displayName(note.from.name)).replace('{count}', String(note.left))
                : t('bundleDlgSwitchDissolves').replace('{from}', displayName(note.from.name))}</span>
            </div>
          ))}
          <label className="flex items-center gap-2 cursor-pointer font-semibold">
            <input type="checkbox" checked={confirmed} disabled={busy} onChange={event => setConfirmed(event.target.checked)} />
            {t('bundleDlgSwitchConfirm')}
          </label>
        </div>
      )}
      {dissolving && <div className={NOTICE}>{t('bundleDlgTooFew')}</div>}
      {error && <div id={dialogError} role="alert" className="text-[var(--crit)]"><ErrorText error={error} /></div>}

      <div className="flex gap-2 justify-end">
        <button type="button" className={catalogActionButton} disabled={busy} onClick={onClose}>{t('cancel')}</button>
        <button type="button" className={`${catalogActionBase} border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)]`}
          disabled={submitDisabled} onClick={() => void submit()}>{submitLabel}</button>
      </div>
    </CatalogActionDialog>
  );
}
