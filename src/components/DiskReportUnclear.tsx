import { useFormatCount, useT } from '../i18n/LanguageContext';

const reasonKey = (reason: string) => reason === 'unreachable' ? 'diskUnclearWhy_unreachable' as const : reason === 'link' ? 'diskUnclearWhy_link' as const : 'diskUnclearWhy_read' as const;

/** Entries the server could not check reliably. Collapsed, no checkboxes: nothing here is ever selected, planned or touched. */
export function DiskReportUnclear({ rows }: { rows: { key: string; path: string; reason: string }[] }) {
  const t = useT();
  const count = useFormatCount();
  if (rows.length === 0) return null;
  return <section>
    <details data-testid="disk-unclear">
      <summary className="cursor-pointer font-semibold">{count(t('diskUnclearSummary'), rows.length)} <span className="ml-1 rounded bg-[var(--panel-2)] px-1.5 py-0.5 text-small font-normal text-[var(--ink-2)]">{t('diskUnclearTag')}</span></summary>
      <p className="my-2 text-small text-[var(--ink-2)]">{t('diskUnclearIntro')}</p>
      <ul className="space-y-1">{rows.map(row => <li key={row.key} className="rounded bg-[var(--panel-2)] p-2 text-small">
        <span className="block break-all font-code">{row.path}</span>
        <span className="block text-[var(--ink-2)]">{t(reasonKey(row.reason))}</span>
      </li>)}</ul>
    </details>
  </section>;
}
