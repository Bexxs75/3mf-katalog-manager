import { useEffect } from 'react';
import { useT } from '../i18n/LanguageContext';
import type { ArchiveOutcome, SkipReason, SkippedFile } from '../types';
import { ReportProblemLink } from '../diagnostics/ReportProblemLink';

interface Props {
  imported: number;
  duplicates: number;
  archives?: ArchiveOutcome[];
  skipped?: SkippedFile[];
  onClose: () => void;
}

// Names shown per reason; a large folder can hold hundreds of broken files.
const MAX_NAMES = 3;

const SKIP_TEXT = {
  empty: 'importSkippedEmpty',
  invalid: 'importSkippedInvalid',
  failed: 'importSkippedFailed',
} as const;

function fileName(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}

export function ImportSummaryBanner({ imported, duplicates, archives, skipped, onClose }: Props) {
  const t = useT();

  const lines: { key: string; text: string; unexpected?: boolean }[] = [];
  let hasProblems = false;
  for (const reason of ['empty', 'invalid', 'failed'] as SkipReason[]) {
    const names = (skipped ?? []).filter((s) => s.reason === reason).map((s) => fileName(s.path));
    if (names.length === 0) continue;
    hasProblems = true;
    let list = names.slice(0, MAX_NAMES).join(', ');
    if (names.length > MAX_NAMES) {
      list += ' ' + t('importSkippedMore').replace('{count}', String(names.length - MAX_NAMES));
    }
    lines.push({
      key: `skipped:${reason}`,
      text: t(SKIP_TEXT[reason]).replace('{count}', String(names.length)).replace('{names}', list),
      unexpected: reason === 'failed',
    });
  }
  if (archives) {
    const existing = archives.reduce((sum, a) => sum + a.existingSkipped, 0);
    const unsafe = archives.reduce((sum, a) => sum + a.unsafeSkipped, 0);
    const blocked = archives.reduce((sum, a) => sum + a.blockedSkipped, 0);
    const deleted = archives.filter((a) => a.archiveDeleted).length;
    if (existing > 0) lines.push({ key: 'existing', text: t('archiveSummaryExistingSkipped').replace('{count}', String(existing)) });
    if (unsafe > 0) lines.push({ key: 'unsafe', text: t('archiveSummaryUnsafeSkipped').replace('{count}', String(unsafe)) });
    if (blocked > 0) lines.push({ key: 'blocked', text: t('archiveSummaryBlockedSkipped').replace('{count}', String(blocked)) });
    if (deleted > 0) lines.push({ key: 'deleted', text: t('archiveSummaryDeleted').replace('{count}', String(deleted)) });
    for (const a of archives) {
      if (a.error) {
        hasProblems = true;
        lines.push({
          key: `failed:${a.path}`,
          text: t('archiveSummaryFailed').replace('{name}', fileName(a.path)).replace('{error}', a.error),
          unexpected: a.unexpected === true,
        });
      } else if (a.deleteError) {
        hasProblems = true;
        lines.push({
          key: `notDeleted:${a.path}`,
          text: t('archiveSummaryNotDeleted').replace('{name}', fileName(a.path)).replace('{error}', a.deleteError),
        });
      }
    }
  }

  // Error messages stay until they are dismissed - otherwise a failed
  // archive would disappear unnoticed after 5 s.
  useEffect(() => {
    if (hasProblems) return;
    const timer = setTimeout(onClose, 5000);
    return () => clearTimeout(timer);
  }, [onClose, hasProblems]);

  return (
    <div className="fixed bottom-4 right-4 z-50 flex items-start gap-2.5 px-3.5 py-2.5 rounded-[4px] border border-[var(--line)] bg-[var(--panel)] shadow-[var(--shadow)] text-[length:var(--font-size-title)] text-[var(--ink)]">
      <div className="flex flex-col gap-1">
        <span>
          {t('importSummaryText')
            .replace('{imported}', String(imported))
            .replace('{duplicates}', String(duplicates))}
        </span>
        {lines.length > 0 && (
          <ul className="text-[length:var(--font-size-meta)] text-[var(--ink-3)]">
            {lines.map((line) => (
              <li key={line.key}>
                {line.text}
                {line.unexpected && <ReportProblemLink />}
              </li>
            ))}
          </ul>
        )}
      </div>
      <span
        onClick={onClose}
        className="w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[length:var(--font-size-meta)] text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
      >
        ✕
      </span>
    </div>
  );
}
