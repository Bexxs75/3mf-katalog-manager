import { useT } from '../i18n/LanguageContext';
import { licenseLabel, licenseUsage, usageLabels } from '../lib/licenses';

export function LicenseBadge({ license, showUnknown = false }: { license: string | null; showUnknown?: boolean }) {
  const t = useT();
  const usage = licenseUsage(license);
  if (usage === 'unknown' && !showUnknown) return null;
  return <span title={licenseLabel(license, t)} className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-caption font-medium max-w-full ${usage === 'commercial' ? 'status-good' : usage === 'private' ? 'status-warn' : 'text-[var(--ink-2)] border-[var(--line)]'}`}>
    {t(usageLabels[usage])}
  </span>;
}
