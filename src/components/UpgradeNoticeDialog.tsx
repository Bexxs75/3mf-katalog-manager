import { Fragment, useEffect, useRef, useState, type ReactNode } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatBytes } from '../i18n/format';
import { ErrorText } from '../diagnostics/ErrorText';
import { toAppError, type AppError } from '../lib/errors';
import { openReleaseUrl } from '../lib/api/update';
import * as api from '../lib/api/upgradeNotice';
import { CatalogActionDialog, catalogActionBase, catalogActionButton } from './CatalogActionDialog';

const primaryButton = `${catalogActionBase} border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)]`;

/** Puts file paths into a translated sentence as code, wherever the translation wants them. */
function withCode(text: string, values: Record<string, string>): ReactNode {
  return text.split(/(\{\w+\})/).map((part, i) => {
    const value = /^\{(\w+)\}$/.exec(part)?.[1];
    return value && value in values
      ? <code key={i} className="font-mono text-small break-all">{values[value]}</code>
      : <Fragment key={i}>{part}</Fragment>;
  });
}

function formatModified(iso: string | null, language: string): string | null {
  if (!iso) return null;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return null;
  return date.toLocaleString(language, { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' });
}

/**
 * One-time information after an update or a manual install changed the catalog
 * database: where the copy of the old state is and how to go back. It never
 * restores anything itself. `wait` holds it back behind the first-run setup, and
 * `onSettled` tells the caller when it is gone or was never needed, so that
 * other start-up questions do not open on top of it.
 */
export function UpgradeNoticeDialog({ wait = false, onSettled }: { wait?: boolean; onSettled?: () => void } = {}) {
  const t = useT();
  const { language } = useLanguage();
  const [notice, setNotice] = useState<api.UpgradeNotice | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const mountedRef = useRef(true);
  const settledRef = useRef(onSettled);
  settledRef.current = onSettled;
  // Set to true again on setup: StrictMode runs setup, cleanup, setup.
  useEffect(() => { mountedRef.current = true; return () => { mountedRef.current = false; }; }, []);
  useEffect(() => {
    if (wait) return;
    if (import.meta.env.VITE_TARGET === 'web') { settledRef.current?.(); return; }
    api.getUpgradeNotice()
      .then(found => {
        if (!mountedRef.current) return;
        if (found) setNotice(found); else settledRef.current?.();
      })
      .catch(e => { console.warn('[startup] reading the upgrade notice failed:', e); if (mountedRef.current) settledRef.current?.(); });
  }, [wait]);
  if (!notice) return null;

  function close() {
    // Closing counts as "understood" even when remembering it fails; the notice
    // would only come back on the next start.
    api.dismissUpgradeNotice().catch(e => console.warn('[startup] remembering the upgrade notice failed:', e));
    setNotice(null);
    settledRef.current?.();
  }
  async function run(action: () => Promise<unknown>) {
    setError(null);
    try { await action(); } catch (e) { if (mountedRef.current) setError(toAppError(e)); }
  }

  const n = notice;
  const failed = n.backupFailed && !n.backup;
  const sr = <span className="sr-only">{t('rwTo')}</span>;
  const arrow = <span aria-hidden="true"> → </span>;
  const version = n.fromVersion && !n.fromVersionDerived
    ? <>{n.fromVersion}{arrow}{sr}{n.toVersion}</>
    : n.fromVersion
      ? t('rwVersionDerived').replace('{to}', n.toVersion).replace('{from}', n.fromVersion)
      : t('rwVersionUnknown').replace('{to}', n.toVersion);
  const schema = n.fromSchema === null
    ? String(n.toSchema)
    : n.fromSchema === n.toSchema
      ? t('rwSchemaUnchanged').replace('{schema}', String(n.toSchema))
      : <>{n.fromSchema}{arrow}{sr}{n.toSchema}</>;
  const modified = n.backup ? formatModified(n.backup.modified, language) : null;
  const folder = n.container ? t('rwInContainer').replace('{path}', n.folder) : n.folder;
  const install = n.fromVersion
    ? (n.previousReleaseUrl ? t('rwStepInstallKnown') : t('rwStepInstallKnownNoLink')).replace('{version}', n.fromVersion)
    : t('rwStepInstallUnknown');
  const termClass = 'text-caption font-semibold text-[var(--ink-3)]';

  return <CatalogActionDialog title={failed ? t('rwTitleFailed') : t('rwTitle')} onClose={close} wide>
    <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1">
      <dt className={termClass}>{t('rwVersionLabel')}</dt>
      <dd>{version}</dd>
      <dt className={termClass}>{t('rwSchemaLabel')}</dt>
      <dd>{schema}</dd>
      {n.backup && <>
        <dt className={termClass}>{t('rwBackupLabel')}</dt>
        <dd><code className="font-mono text-small break-all">{n.backup.fileName}</code><br />
          {[modified, formatBytes(n.backup.sizeBytes, language)].filter(Boolean).join(' · ')}</dd>
        <dt className={termClass}>{t(n.container ? 'rwLocationLabel' : 'rwFolderLabel')}</dt>
        <dd><code className="font-mono text-small break-all">{folder}</code></dd>
      </>}
    </dl>
    {failed ? <>
      <div role="alert" className="p-3 border border-[var(--warn)] rounded">{t('migrationBackupWarning')}</div>
      <p className="text-small text-[var(--ink-2)]">{t('rwFailedNote')}</p>
    </> : <>
      {n.container
        ? <p>{t('rwContainerNote')}</p>
        : <section>
          <h3 className={`${termClass} mb-1`}>{t('rwStepsTitle')}</h3>
          <ol className="list-decimal pl-5 space-y-1">
            <li>{t('rwStepClose')}</li>
            <li>{install}</li>
            <li>{n.backup && withCode(t('rwStepCopy'), { backup: n.backup.path, catalog: n.catalogFile })}</li>
          </ol>
        </section>}
      <div role="note" className="p-3 border border-[var(--warn)] rounded">
        <p className="font-semibold">{t('rwConsequencesTitle')}</p>
        <ul className="list-disc pl-5 mt-1 space-y-1">
          <li>{t('rwConsequenceLost')}</li>
          <li>{t('rwConsequenceFiles')}</li>
        </ul>
      </div>
    </>}
    {error && <div role="alert" className="p-3 border border-[var(--crit)] rounded text-body"><ErrorText error={error} /></div>}
    <div className="flex flex-wrap items-center gap-2">
      {!failed && !n.container && <button className={catalogActionButton} onClick={() => void run(api.openUpdateBackupFolder)}>{t('rwOpenFolder')}</button>}
      {n.previousReleaseUrl && !failed && <button className={catalogActionButton} onClick={() => void run(() => openReleaseUrl(n.previousReleaseUrl!))}>{t('rwDownloadPrevious')}</button>}
      <span className="flex-1" />
      <button className={primaryButton} data-initial-focus onClick={close}>{t('rwUnderstood')}</button>
    </div>
  </CatalogActionDialog>;
}
