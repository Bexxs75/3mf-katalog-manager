import { useEffect } from 'react';
import type { ModelFile } from '../types';
import { useT } from '../i18n/LanguageContext';
import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { isReadOnly } from '../lib/readOnly';
import { loadNativeDragEnabled, startNativeDrag, useNativeDragEnabled } from '../lib/nativeDrag';
import { reportMetadataError } from '../lib/metadataErrors';

export function NativeDragHandle({ model, selectedIds, className = '' }: {
  model: ModelFile; selectedIds?: ReadonlySet<string>; className?: string;
}) {
  const t = useT();
  const environment = useRuntimeEnvironment();
  const supported = !isReadOnly() && !environment.container && !environment.web;
  const enabled = useNativeDragEnabled();
  useEffect(() => { if (supported) void loadNativeDragEnabled(); }, [supported]);
  if (!supported || !enabled || model.deletedAt) return null;
  const extension = model.path.split('.').pop()?.toLowerCase();
  if (extension === 'step' || extension === 'stp') return <span className={`text-caption ${className}`} title={t('dgStepHint')}>{t('dgStepHint')}</span>;
  if (!extension || !['3mf', 'stl', 'obj'].includes(extension)) return null;
  return <button type="button" className={`rounded border border-[var(--line)] bg-[var(--panel)] px-1 text-[var(--ink-2)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)] ${className}`}
    aria-label={t('dgHandle')} title={t('dgHandle')}
    onClick={event => event.stopPropagation()}
    onDoubleClick={event => event.stopPropagation()}
    onMouseDown={event => {
      event.stopPropagation();
      if (event.button !== 0) return;
      event.preventDefault();
      const ids = selectedIds?.has(model.id) ? [...selectedIds] : [model.id];
      void startNativeDrag(ids).catch(reportMetadataError);
    }}>
    <span aria-hidden="true">⠿</span>
  </button>;
}
