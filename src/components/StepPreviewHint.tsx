import { useLanguage, useT } from '../i18n/LanguageContext';
import { openStepDownload } from '../lib/api/stepPreview';

/**
 * Shown in place of the 3D viewer for STEP files when this build has no
 * STEP support at all. Calling the geometry command here would just be a
 * guaranteed, misleading "something went wrong" - this explains the build
 * choice instead and offers the switch.
 */
export function StepPreviewHint() {
  const t = useT();
  const { language } = useLanguage();

  return (
    <div className="relative w-full h-full">
      <div className="absolute inset-0 grid place-items-center p-4">
        <div className="max-w-[300px] bg-[var(--panel)] border border-[var(--line)] rounded-[10px] shadow-[var(--shadow)] px-4 py-3.5">
          <div className="text-[13.5px] font-semibold mb-1.5">{t('stepHintTitle')}</div>
          <p className="text-[12.5px] text-[var(--ink-2)] mb-2.5">
            {t('stepHintTextBefore')}
            <strong className="text-[var(--ink)]">{t('stepHintVariantBold')}</strong>
            {t('stepHintTextAfter')}
          </p>
          <button
            onClick={() =>
              openStepDownload(language).catch((e) =>
                console.warn('[step-hint] could not open the download page:', e),
              )
            }
            className="h-8 px-3 rounded-md border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer"
          >
            {t('stepHintButton')}
          </button>
        </div>
      </div>
    </div>
  );
}
