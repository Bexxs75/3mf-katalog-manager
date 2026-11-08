import { Icon } from './Icon';
import { useEffect, useRef, useState } from 'react';
import type { ModelFile } from '../types';
import { useT } from '../i18n/LanguageContext';
import { useRuntimeEnvironment } from '../hooks/useRuntimeEnvironment';
import { isReadOnly } from '../lib/readOnly';
import { loadNativeDragEnabled, beginDragOut, nativeDragIdsFor, MAX_NATIVE_DRAG_FILES, useNativeDragEnabled } from '../lib/nativeDrag';
import { reportMetadataError } from '../lib/metadataErrors';

export function NativeDragHandle({ model, selectedIds, selectionModels = [], className = '' }: {
  model: ModelFile; selectedIds?: ReadonlySet<string>; selectionModels?: readonly ModelFile[]; className?: string;
}) {
  const t = useT();
  const [overLimit, setOverLimit] = useState(false);
  const environment = useRuntimeEnvironment();
  const supported = !isReadOnly() && !environment.container && !environment.web;
  const enabled = useNativeDragEnabled();
  const cancelGesture = useRef<(() => void) | undefined>(undefined);
  useEffect(() => () => cancelGesture.current?.(), [supported, enabled, model.id]);
  useEffect(() => { if (supported) void loadNativeDragEnabled(); }, [supported]);
  if (!supported || !enabled || model.deletedAt) return null;
  const extension = model.path.split('.').pop()?.toLowerCase();
  if (extension === 'step' || extension === 'stp') return <span className={`text-caption ${className}`} title={t('dgStepHint')}>{t('dgStepHint')}</span>;
  if (!extension || !['3mf', 'stl', 'obj'].includes(extension)) return null;
  return <><button type="button" className={`min-w-6 min-h-6 shrink-0 rounded border border-[var(--line)] bg-[var(--panel)] px-1 text-[var(--ink-2)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)] ${className}`}
    tabIndex={-1} aria-hidden="true" title={t('dgHandle')}
    onClick={event => event.stopPropagation()}
    onDoubleClick={event => event.stopPropagation()}
    onMouseDown={event => {
      event.stopPropagation();
      if (event.button !== 0) return;
      event.preventDefault();
      cancelGesture.current?.();
      const { clientX, clientY } = event;
      const cancel = () => {
        window.removeEventListener('mousemove', move);
        window.removeEventListener('mouseup', cancel);
        window.removeEventListener('blur', cancel);
        cancelGesture.current = undefined;
      };
      const move = (movement: MouseEvent) => {
        if ((movement.buttons & 1) === 0) { cancel(); return; }
        if (Math.hypot(movement.clientX - clientX, movement.clientY - clientY) <= 4) return;
        cancel();
        const ids = nativeDragIdsFor(model, selectedIds?.has(model.id)
          ? selectionModels.filter(file => selectedIds.has(file.id)) : undefined);
        setOverLimit(ids.length > MAX_NATIVE_DRAG_FILES);
        if (ids.length === 0 || ids.length > MAX_NATIVE_DRAG_FILES) return;
        void beginDragOut(ids).catch(reportMetadataError);
      };
      cancelGesture.current = cancel;
      window.addEventListener('mousemove', move);
      window.addEventListener('mouseup', cancel);
      window.addEventListener('blur', cancel);
    }}>
    <Icon name="external" size={16} className="mx-auto" />
  </button>{overLimit && <span role="alert" className="text-caption text-[var(--crit)]">{t('dgLimit')}</span>}</>;
}
