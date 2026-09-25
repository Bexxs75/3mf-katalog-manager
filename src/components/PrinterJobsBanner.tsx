import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatStockG } from '../i18n/format';
import type { PrinterJob } from '../types';

interface Props {
  jobs: PrinterJob[];
  onReview: () => void;
}

/** Hinweis über der Lagertabelle, nur wenn Drucke zu bestätigen sind. */
export function PrinterJobsBanner({ jobs, onReview }: Props) {
  const t = useT();
  const { language } = useLanguage();
  if (jobs.length === 0) return null;
  const printers = [...new Set(jobs.map((j) => j.printerName))].join(', ');
  const total = jobs.reduce((sum, j) => sum + (j.grams ?? 0), 0);
  const title = jobs.length === 1 ? t('printerJobsBannerOne') : t('printerJobsBanner').replace('{count}', String(jobs.length));
  return (
    <div role="status" className="flex items-center gap-3 rounded-lg border border-[var(--accent)] bg-[var(--accent-soft)] px-3.5 py-2.5">
      <span className="grid place-items-center w-[26px] h-[26px] flex-none rounded-md bg-[var(--accent)] text-[var(--accent-ink)] font-bold text-[13px]">
        {jobs.length}
      </span>
      <p className="flex-1 min-w-0 text-[13px]">
        {title}
        <small className="block text-[11.5px] text-[var(--ink-2)]">
          {t('printerJobsBannerDetail').replace('{printers}', printers).replace('{grams}', formatStockG(total, language))}
        </small>
      </p>
      <button
        type="button"
        onClick={onReview}
        className="h-8 px-3 rounded-md border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12px] font-bold cursor-pointer"
      >
        {t('printerJobsReview')}
      </button>
    </div>
  );
}
