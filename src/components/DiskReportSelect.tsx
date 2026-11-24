import { useFormatCount, useT } from '../i18n/LanguageContext';
import type { DiskReport } from '../lib/api/diskSync';
import { groupMissingFolders, newKey, type DiskSection, type DiskSelection } from './DiskReportModel';
import { DiskReportUnclear } from './DiskReportUnclear';

const link = 'text-small text-[var(--accent)] cursor-pointer hover:underline focus-visible:outline-2 focus-visible:outline-[var(--accent)] disabled:opacity-50 disabled:cursor-default disabled:no-underline';

interface Row { key: string; root: string | null; path: string; note?: string; disabled?: boolean }

function Section({ id, heading, countText, rows, selected, hint, onToggle, onAll, onNone }: {
  id: DiskSection; heading: string; countText: string; rows: Row[]; selected: ReadonlySet<string>; hint?: string;
  onToggle: (key: string) => void; onAll: () => void; onNone: () => void;
}) {
  const t = useT();
  const headingId = `disk-report-${id}`;
  const offered = rows.filter(row => !row.disabled);
  return <section aria-labelledby={headingId}>
    <h3 id={headingId} className="ui-label mb-1">{heading}</h3>
    <p className="mb-1 text-small">{countText}</p>
    {offered.length > 0 && <div className="mb-1 flex gap-3">
      <button type="button" className={link} aria-label={t('diskSyncSelectAllIn').replace('{section}', heading)} disabled={offered.every(row => selected.has(row.key))} onClick={onAll}>{t('diskSyncSelectAll')}</button>
      <button type="button" className={link} aria-label={t('diskSyncSelectNoneIn').replace('{section}', heading)} disabled={selected.size === 0} onClick={onNone}>{t('diskSyncSelectNone')}</button>
    </div>}
    <ul className="space-y-1">{rows.map(row => <li key={row.key} className="rounded bg-[var(--panel-2)] p-2 text-small">
      <label className={`flex items-start gap-2 ${row.disabled ? '' : 'cursor-pointer'}`}>
        <input type="checkbox" className="mt-0.5" checked={selected.has(row.key)} disabled={row.disabled} onChange={() => onToggle(row.key)} />
        <span className="min-w-0">
          <span className="block break-all font-code">{row.path}</span>
          {row.root && <span className="block text-[var(--ink-3)] break-all">{row.root}</span>}
          {row.note && <span className="block text-[var(--ink-2)]">{row.note}</span>}
        </span>
      </label>
    </li>)}</ul>
    {hint && <p className="mt-1 text-small text-[var(--ink-2)]">{hint}</p>}
  </section>;
}

/** The three lists of the report with a checkbox per row. Unclear rows never reach this list (the server leaves them out). */
export function DiskReportSelect({ report, selection, onToggle, onAll, onNone, onFolder }: {
  report: DiskReport; selection: DiskSelection;
  onToggle: (section: DiskSection, key: string) => void; onAll: (section: DiskSection) => void; onNone: (section: DiskSection) => void;
  onFolder: (ids: string[], on: boolean) => void;
}) {
  const t = useT();
  const count = useFormatCount();
  const showRoot = report.roots.length > 1;
  const { folders, single } = groupMissingFolders(report);
  const unclear = report.unclear ?? [];
  const rootName = (index: number) => showRoot ? report.roots[index]?.name ?? null : null;
  const handlers = (id: DiskSection) => ({ onToggle: (key: string) => onToggle(id, key), onAll: () => onAll(id), onNone: () => onNone(id) });
  return <>
    <ul className="flex flex-wrap gap-1.5" aria-label={t('diskReportTitle')}>
      <li className="rounded-full bg-[var(--panel-2)] px-2.5 py-0.5 text-small">{count(t('diskReportNew'), report.newFiles.length)}</li>
      <li className="rounded-full bg-[var(--panel-2)] px-2.5 py-0.5 text-small">{count(t('diskReportMissing'), report.missingModels.length)}</li>
      <li className="rounded-full bg-[var(--panel-2)] px-2.5 py-0.5 text-small">{count(t('diskReportCompanions'), report.danglingCompanions.length)}</li>
      {unclear.length > 0 && <li className="rounded-full bg-[var(--panel-2)] px-2.5 py-0.5 text-small">{count(t('diskUnclearChip'), unclear.length)}</li>}
    </ul>
    {report.newFiles.length > 0 && <Section id="new" heading={t('diskSyncNewHeading')} countText={count(t('diskReportNew'), report.newFiles.length)} selected={selection.new}
      rows={report.newFiles.map(f => ({ key: newKey(f), root: rootName(f.root), path: f.relativePath, note: t('diskApplyNewAction') }))} {...handlers('new')} />}
    {folders.length > 0 && <section aria-labelledby="disk-report-folders">
      <h3 id="disk-report-folders" className="ui-label mb-1">{t('diskFolderHeading')}</h3>
      <ul className="space-y-1">{folders.map(folder => {
        const ids = folder.models.map(m => m.id);
        return <li key={folder.key} className="rounded bg-[var(--panel-2)] p-2 text-small">
          <label className="flex items-start gap-2 cursor-pointer">
            <input type="checkbox" className="mt-0.5" checked={ids.every(id => selection.missing.has(id))} onChange={event => onFolder(ids, event.target.checked)} />
            <span className="min-w-0">
              <span className="block"><b>{t('diskFolderLabel')}</b> <span className="break-all font-code">{showRoot ? `${rootName(folder.root)}: ` : ''}{folder.path}</span></span>
              <span className="block text-[var(--ink-2)]">{count(t('diskFolderModels'), folder.models.length)}</span>
              <span className="block break-all font-code text-[var(--ink-3)]">{folder.models.map(m => m.relativePath.slice(folder.path.length + 1)).join(' · ')}</span>
            </span>
          </label>
        </li>;
      })}</ul>
    </section>}
    {single.length > 0 && <Section id="missing" heading={t('diskSyncMissingHeading')} countText={count(t('diskReportMissing'), single.length)} selected={selection.missing}
      rows={single.map(m => ({ key: m.id, root: rootName(m.root), path: m.relativePath, note: t('diskApplyMissingAction') }))} hint={t('diskApplyMissingHint')} {...handlers('missing')} />}
    {report.danglingCompanions.length > 0 && <Section id="companions" heading={t('diskReportCompanionHeading')} countText={count(t('diskReportCompanions'), report.danglingCompanions.length)} selected={selection.companions}
      rows={report.danglingCompanions.map(c => ({ key: c.id, root: rootName(c.root), path: c.relativePath, disabled: c.reason === 'notFile', note: c.reason === 'notFile' ? t('diskReportNotFile') : t('diskApplyCompanionAction') }))} {...handlers('companions')} />}
  <DiskReportUnclear rows={unclear.map((u, i) => ({ key: `${i}`, path: showRoot && u.root !== undefined ? `${rootName(u.root)}: ${u.relativePath}` : u.relativePath, reason: u.reason }))} />
  </>;
}
