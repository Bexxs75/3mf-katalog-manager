import { useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { useImportLock } from '../hooks/ImportLockContext';
import { useStlEntry } from '../hooks/useStlEntry';
import { modelFileType } from '../lib/stlExport';
import { StlExportDialog } from './StlExportDialog';

/** "Save as STL" with its own dialog, for the detail page and the detail panel. Renders nothing where the entry does not apply. */
export function SaveAsStlButton({ model, className }: {
  model: { id: string; name: string; fileType?: string; path?: string }; className: string;
}) {
  const t = useT();
  const { lockProps } = useImportLock();
  const state = useStlEntry(model);
  const [open, setOpen] = useState(false);
  if (state === 'hidden') return null;
  const disabled = state === 'disabled';
  return <>
    <button {...lockProps} disabled={lockProps.disabled || disabled} onClick={() => setOpen(true)}
      aria-describedby={disabled ? `stl-hint-${model.id}` : undefined}
      title={disabled ? t('stStepNoPreview') : undefined}
      className={className}>
      {t('stSaveAsStl')}
    </button>
    {disabled && <span id={`stl-hint-${model.id}`} className="sr-only">{t('stStepNoPreview')}</span>}
    {open && <StlExportDialog key={model.id} model={{ id: model.id, name: model.name, fileType: modelFileType(model) }} onClose={() => setOpen(false)} />}
  </>;
}

