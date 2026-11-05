import { useEffect, useState } from 'react';
import type { ModelFile } from '../types';
import { useT } from '../i18n/LanguageContext';
import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { isReadOnly } from '../lib/readOnly';
import { loadNativeDragEnabled, startNativeDrag, useNativeDragEnabled } from '../lib/nativeDrag';
import { reportMetadataError } from '../lib/metadataErrors';

export function NativeDragHandle({ model, selectedIds, selectionModels = [], className = '' }: {
  model: ModelFile; selectedIds?: ReadonlySet<string>; selectionModels?: readonly ModelFile[]; className?: string;
}) {
  const t = useT();
  const [overLimit, setOverLimit] = useState(false);
  const environment = useRuntimeEnvironment();
  const supported = !isReadOnly() && !environment.container && !environment.web;
  const enabled = useNativeDragEnabled();
  useEffect(() => { if (supported) void loadNativeDragEnabled(); }, [supported]);
  if (!supported || !enabled || model.deletedAt) return null;
  const extension = model.path.split('.').pop()?.toLowerCase();
  if (extension === 'step' || extension === 'stp') return <span className={`text-caption ${className}`} title={t('dgStepHint')}>{t('dgStepHint')}</span>;
  if (!extension || !['3mf', 'stl', 'obj'].includes(extension)) return null;
  return <><button type="button" className={`rounded border border-[var(--line)] bg-[var(--panel)] px-1 text-[var(--ink-2)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)] ${className}`}
    tabIndex={-1} aria-hidden="true" title={t('dgHandle')}
    onClick={event => event.stopPropagation()}
    onDoubleClick={event => event.stopPropagation()}
    onMouseDown={event => {
      event.stopPropagation();
      if (event.button !== 0) return;
      event.preventDefault();
      const ids = selectedIds?.has(model.id)
        ? selectionModels.filter(file => selectedIds.has(file.id) && !file.deletedAt && /\.(3mf|stl|obj)$/i.test(file.path)).map(file => file.id)
        : [model.id];
      setOverLimit(ids.length > 64);
      if (ids.length === 0 || ids.length > 64) return;
      void startNativeDrag(ids).catch(reportMetadataError);
    }}>
    <span aria-hidden="true">⠿</span>
  </button>{overLimit && <span role="alert" className="text-caption text-[var(--crit)]">{t('dgLimit')}</span>}</>;
}
