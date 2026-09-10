import { useState } from 'react';
import type { CatalogIssues } from '../types';
import { useT } from '../i18n/LanguageContext';

interface Props {
  issues: CatalogIssues;
  onClose: () => void;
  onDelete: (fileIds: string[]) => void;
}

function initialSelection(issues: CatalogIssues): Set<string> {
  const checked = new Set<string>();
  issues.orphaned.forEach((m) => checked.add(m.id));
  issues.duplicateGroups.forEach((group) => {
    group.slice(1).forEach((m) => checked.add(m.id));
  });
  return checked;
}

export function CatalogCleanupDialog({ issues, onClose, onDelete }: Props) {
  const t = useT();
  const [checked, setChecked] = useState<Set<string>>(() => initialSelection(issues));

  const toggle = (id: string) => {
    setChecked((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const hasIssues = issues.orphaned.length > 0 || issues.duplicateGroups.length > 0;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-[480px] max-h-[80vh] flex flex-col bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)]">
        <div className="flex-none px-4 py-3 border-b border-[var(--line)] flex items-center justify-between">
          <span className="text-[14px] font-semibold">{t('cleanupDialogTitle')}</span>
          <span onClick={onClose} className="cursor-pointer text-[var(--ink-3)] hover:text-[var(--accent)]">
            ✕
          </span>
        </div>

        <div className="flex-1 overflow-y-auto px-4 py-3">
          {!hasIssues && <div className="text-[12.5px] text-[var(--ink-3)]">{t('cleanupNoIssues')}</div>}

          {issues.orphaned.length > 0 && (
            <div className="pb-4">
              <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] pb-2">
                {t('cleanupOrphanedHeading')}
              </div>
              {issues.orphaned.map((model) => (
                <label key={model.id} className="flex items-center gap-2 py-1 text-[12.5px] cursor-pointer">
                  <input type="checkbox" checked={checked.has(model.id)} onChange={() => toggle(model.id)} />
                  <span className="flex-1 truncate">{model.name}</span>
                  <span className="font-mono-ui text-[10.5px] text-[var(--ink-3)] truncate max-w-[160px]">
                    {model.path}
                  </span>
                </label>
              ))}
            </div>
          )}

          {issues.duplicateGroups.map((group, groupIndex) => (
            <div key={groupIndex} className="pb-4">
              <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] pb-2">
                {t('cleanupDuplicateGroupHeading')} {groupIndex + 1}
              </div>
              {group.map((model, index) => (
                <label
                  key={model.id}
                  className={`flex items-center gap-2 py-1 text-[12.5px] ${index === 0 ? 'opacity-60' : 'cursor-pointer'}`}
                >
                  <input
                    type="checkbox"
                    checked={checked.has(model.id)}
                    disabled={index === 0}
                    onChange={() => toggle(model.id)}
                  />
                  <span className="flex-1 truncate">{model.name}</span>
                  {index === 0 && (
                    <span className="font-mono-ui text-[10px] text-[var(--ink-3)]">{t('cleanupKeepOldest')}</span>
                  )}
                </label>
              ))}
            </div>
          ))}
        </div>

        <div className="flex-none px-4 py-3 border-t border-[var(--line)] flex justify-end gap-2">
          <button
            onClick={onClose}
            className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {t('cancel')}
          </button>
          <button
            onClick={() => onDelete(Array.from(checked))}
            disabled={checked.size === 0}
            className={`h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold ${
              checked.size === 0 ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer'
            }`}
          >
            {t('cleanupDeleteSelected')}
          </button>
        </div>
      </div>
    </div>
  );
}
