import { useRef, useState } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatBytes, formatPrintDuration, formatWeightG } from '../i18n/format';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { useSlicerReports } from '../hooks/SlicerReportsContext';
import { isChoosableModel, type SlicerReport } from '../lib/slicerReports';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';
import { ModelPicker, type ModelOption } from './ModelPicker';

interface Props {
  /** Catalog entries; G-code and resin entries are left out of the choice. */
  models: Array<ModelOption & { fileType?: string }>;
  /** Moves the reported files into the catalog (the normal import, explicitly asked for). */
  onImportPaths: (paths: string[]) => Promise<void>;
}

/**
 * The reports of the slicers with the suggested model. The suggestion is only that: "Assign" remembers the
 * link (the print connection then knows the model), "Also add to the catalog" is a separate tick that says
 * it moves the file. Nothing happens without a click.
 */
export function SlicerReportInbox({ models, onImportPaths }: Props) {
  const t = useT();
  const { language } = useLanguage();
  const { reports, open, resolve, closeInbox } = useSlicerReports();
  const [chosen, setChosen] = useState<Record<string, ModelOption | null>>({});
  const [include, setInclude] = useState<Record<string, boolean>>({});
  const [picking, setPicking] = useState<string | null>(null);
  const [status, setStatus] = useState('');
  const [error, setError] = useState<{ text: string; error: AppError } | null>(null);
  const [busy, setBusy] = useState(false);
  const statusRef = useRef<HTMLParagraphElement>(null);
  const pickAnchor = useRef<HTMLButtonElement | null>(null);
  const choosable = models.filter((m) => isChoosableModel(m.fileType));
  const handled = reports.filter((r) => r.state !== 'open');

  const modelOf = (r: SlicerReport): ModelOption | null => {
    if (r.id in chosen) return chosen[r.id];
    return r.suggestion ? { id: r.suggestion.fileId, name: r.suggestion.fileName } : null;
  };
  const afterAction = (message: string) => {
    setStatus(message);
    // Focus goes to the next report, or to the status line when none is left.
    window.setTimeout(() => {
      const next = document.querySelector<HTMLElement>('[data-sr-report] [data-sr-first]');
      (next ?? statusRef.current)?.focus();
    }, 0);
  };

  const assign = async (r: SlicerReport) => {
    const model = modelOf(r);
    if (!model) return;
    setBusy(true);
    setError(null);
    try {
      await resolve(r.id, 'link', model.id);
    } catch (e) {
      setError({ text: t('srActionFailed'), error: toAppError(e) });
      setBusy(false);
      return;
    }
    const withImport = include[r.id] === true && !r.isTest;
    let message = (withImport ? t('srAssignedIncluded') : `${t('srAssigned')} ${t('srAssignedKept')}`)
      .replace('{file}', r.fileName).replace('{model}', model.name);
    if (withImport) {
      try {
        await onImportPaths([r.reportedPath]);
      } catch (e) {
        setError({ text: t('srImportFailed'), error: toAppError(e) });
        message = t('srAssigned').replace('{file}', r.fileName).replace('{model}', model.name);
      }
    }
    setBusy(false);
    afterAction(message);
  };

  const dismiss = async (r: SlicerReport) => {
    setBusy(true);
    setError(null);
    try {
      await resolve(r.id, 'dismiss');
      afterAction(t('srDismissed').replace('{file}', r.fileName));
    } catch (e) {
      setError({ text: t('srActionFailed'), error: toAppError(e) });
    }
    setBusy(false);
  };

  const time = (iso: string) => {
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? '' : new Intl.DateTimeFormat(language, { dateStyle: 'medium', timeStyle: 'short' }).format(d);
  };
  const facts = (r: SlicerReport) => [
    r.printSeconds !== null ? formatPrintDuration(r.printSeconds, language, t) : null,
    r.weightG !== null ? formatWeightG(Math.round(r.weightG * 10) / 10, language) : null,
    r.filamentTypes.length > 0 ? r.filamentTypes.join(', ') : null,
    r.slicer,
    r.fileSizeBytes > 0 ? formatBytes(r.fileSizeBytes, language) : null,
    r.reportedAt ? t('srReportedAt').replace('{time}', time(r.reportedAt)) : null,
  ].filter((x): x is string => x !== null && x !== '').join(' · ');

  return (
    <CatalogActionDialog title={t('srInboxTitle')} wide busy={busy} onClose={closeInbox}>
      {open.length === 0 && (
        <div className="text-center py-6 text-[var(--ink-3)]">
          <div className="font-semibold text-[var(--ink)]">{t('srEmptyTitle')}</div>
          <p className="mt-1.5 text-small">{t('srEmptyText')}</p>
        </div>
      )}
      <ul className="space-y-3">
        {open.map((r, index) => {
          const model = modelOf(r);
          const sure = r.suggestion?.sure === true && !(r.id in chosen);
          const withInclude = include[r.id] === true;
          return (
            <li key={r.id} data-sr-report className="p-3 border border-[var(--line)] rounded-[4px] space-y-2">
              <div className="flex items-baseline gap-2 flex-wrap">
                <span className="font-semibold [overflow-wrap:anywhere]">{r.fileName}</span>
                {r.isTest && <span className="px-1.5 rounded border border-[var(--line-strong)] text-caption text-[var(--ink-2)]">{t('srTestBadge')}</span>}
              </div>
              <div className="text-caption text-[var(--ink-3)]">{facts(r)}</div>
              <div className="text-caption text-[var(--ink-3)] font-mono [overflow-wrap:anywhere]">{r.reportedPath}</div>
              {!r.isTest && (
                <div className="text-small">
                  {model
                    ? (r.id in chosen
                      ? t('srChosen').replace('{model}', model.name)
                      : <>{t('srSuggestion').replace('{model}', model.name)} <b>({sure ? t('srSure') : t('srUnsure')})</b></>)
                    : t('srNoSuggestion')}
                </div>
              )}
              <div className="flex flex-wrap gap-2">
                {!r.isTest && model && (
                  <button type="button" data-sr-first={index === 0 ? '' : undefined} data-initial-focus={index === 0 ? '' : undefined}
                    className={`${catalogActionButton} !border-[var(--accent)]`} disabled={busy} onClick={() => void assign(r)}>{t('srAssign')}</button>
                )}
                {!r.isTest && (
                  <button type="button" className={catalogActionButton} disabled={busy}
                    ref={(el) => { if (picking === r.id) pickAnchor.current = el; }} onClick={() => setPicking(r.id)}>{t('srPick')}</button>
                )}
                <button type="button" data-sr-first={r.isTest && index === 0 ? '' : undefined} data-initial-focus={r.isTest && index === 0 ? '' : undefined}
                  className={catalogActionButton} disabled={busy} onClick={() => void dismiss(r)}>{t('srDismiss')}</button>
              </div>
              {picking === r.id && (
                <ModelPicker models={choosable} anchorRef={pickAnchor} onClose={() => setPicking(null)}
                  onChange={(fileId) => {
                    setPicking(null);
                    if (fileId === null) { setChosen(({ [r.id]: _dropped, ...rest }) => rest); return; }
                    const picked = choosable.find((m) => m.id === fileId);
                    if (picked) setChosen((c) => ({ ...c, [r.id]: { id: picked.id, name: picked.name } }));
                  }} />
              )}
              {!r.isTest && model && (
                <>
                  <label className="flex items-start gap-2 text-small cursor-pointer">
                    <input type="checkbox" className="mt-0.5" checked={withInclude} onChange={(e) => setInclude((s) => ({ ...s, [r.id]: e.target.checked }))} />
                    <span>{t('srInclude')} <b>{t('srIncludeMoves')}</b></span>
                  </label>
                  {withInclude && <div className="p-2 border border-[var(--warn)] rounded text-caption" role="note"><b>!</b> {t('srIncludeNote')}</div>}
                </>
              )}
            </li>
          );
        })}
      </ul>
      {handled.length > 0 && (
        <details>
          <summary className="cursor-pointer text-small text-[var(--ink-2)]">{t('srHandled')} ({handled.length})</summary>
          <ul className="mt-1.5 space-y-0.5 text-caption text-[var(--ink-3)]">
            {handled.map((r) => (
              <li key={r.id} className="[overflow-wrap:anywhere]">
                {r.fileName} – {r.state === 'linked' && r.model ? t('srHandledLinked').replace('{model}', r.model.fileName) : t('srHandledDismissed')}
              </li>
            ))}
          </ul>
        </details>
      )}
      <p ref={statusRef} tabIndex={-1} role="status" aria-live="polite" className="text-small text-[var(--ink-2)] outline-none">{status}</p>
      {error && <p role="alert" className="text-small text-[var(--crit)]">{error.text} <ErrorText error={error.error} /></p>}
      <div className="flex justify-end">
        <button type="button" className={catalogActionButton} disabled={busy} onClick={closeInbox}>{t('srInboxClose')}</button>
      </div>
    </CatalogActionDialog>
  );
}
