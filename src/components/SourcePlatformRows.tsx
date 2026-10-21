import type { ModelFile } from '../types';
import { useT } from '../i18n/LanguageContext';
import { platformDisplayName } from '../lib/sourcePlatform';

type Variant = 'compact' | 'comfort' | 'page';

// Each view keeps its own row look so these rows blend in with their neighbours.
const STYLES: Record<Variant, { row: string; label: string; value: string }> = {
  compact: {
    row: 'flex items-baseline gap-3 py-1.5 border-b border-[var(--line)]',
    label: 'flex-none w-[108px] text-small text-[var(--ink-2)]',
    value: 'flex-1 min-w-0 text-right text-small font-medium',
  },
  comfort: {
    row: 'flex justify-between gap-3 py-2 border-b border-[var(--line)]',
    label: 'flex-none text-[var(--ink-2)]',
    value: 'min-w-0 text-right font-semibold',
  },
  page: {
    row: 'flex justify-between items-baseline gap-4 py-2 border-b border-[var(--line)] text-body',
    label: 'flex-none text-[var(--ink-3)]',
    value: 'min-w-0 text-right font-medium',
  },
};

type Props = {
  model: Pick<ModelFile, 'sourcePlatform' | 'sourceModelId' | 'sourceProfileId'>;
  variant: Variant;
};

// Platform, model ID and profile ID read from the 3MF file. Nothing is shown
// without a platform. All values are foreign data: rendered as React text only,
// and wrapped anywhere so a long ID cannot widen the panel. No CSS truncation:
// the full text has to stay readable, selectable and visible to tests.
export function SourcePlatformRows({ model, variant }: Props) {
  const t = useT();
  if (!model.sourcePlatform) return null;
  const s = STYLES[variant];
  const idClass = `${s.value} font-mono select-text [overflow-wrap:anywhere] break-all`;
  return (
    <>
      <div className={s.row} data-testid="source-platform-row">
        <span className={s.label}>{t('metaPlatform')}</span>
        <span className={s.value}>
          <span className="inline-block max-w-full rounded-full px-2.5 py-0.5 bg-[var(--accent-soft)] text-[var(--accent)] text-small font-semibold [overflow-wrap:anywhere]">
            {platformDisplayName(model.sourcePlatform)}
          </span>
        </span>
      </div>
      {model.sourceModelId ? (
        <div className={s.row} data-testid="source-model-id-row">
          <span className={s.label}>{t('metaModelId')}</span>
          <span className={idClass}>{model.sourceModelId}</span>
        </div>
      ) : null}
      {model.sourceProfileId ? (
        <div className={s.row} data-testid="source-profile-id-row">
          <span className={s.label}>{t('metaProfileId')}</span>
          <span className={idClass}>{model.sourceProfileId}</span>
        </div>
      ) : null}
    </>
  );
}
