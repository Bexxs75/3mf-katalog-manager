import type { FilamentCheck, FilamentCheckStatus } from '../types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { STATUS_SYMBOL, queueTooltip } from '../lib/filamentCheck';

const TONE: Record<FilamentCheckStatus, string> = {
  ok: 'text-[var(--good)] bg-[var(--good-soft)] border-transparent',
  swap: 'text-[var(--warn)] bg-[var(--warn-soft)] border-transparent',
  short: 'text-[var(--crit)] bg-[var(--crit-soft)] border-transparent',
  unknown: 'text-[var(--unk)] bg-[var(--unk-soft)] border-transparent',
  no_data: 'text-[var(--ink-3)] bg-transparent border-[var(--line)]',
};

export function QueueFilamentSymbol({ check }: { check: FilamentCheck | undefined }) {
  const t = useT();
  const { language } = useLanguage();
  if (!check) return null;
  return (
    <span
      title={queueTooltip(check, t, language)}
      className={`inline-grid place-items-center w-[18px] h-[18px] rounded-[5px] border font-mono-ui text-[11px] font-semibold flex-none ${TONE[check.status]}`}
    >
      {STATUS_SYMBOL[check.status]}
    </span>
  );
}
