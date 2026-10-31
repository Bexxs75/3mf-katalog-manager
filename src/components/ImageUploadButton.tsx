import { useRef, type CSSProperties } from 'react';
import { useT } from '../i18n/LanguageContext';
import { isReadOnly } from '../lib/readOnly';
import { useCanEditMetadata } from '../lib/webSession';

/**
 * Desktop: the click starts the backend's native dialog (no file is passed).
 * Web: the browser has no such dialog, so the click opens a file input and the picked
 * file is handed over; without write mode the button is disabled and nothing is sent.
 */
export function ImageUploadButton({ className, style, onUpload }: {
  className: string;
  style?: CSSProperties;
  onUpload: (file?: File) => void | Promise<void>;
}) {
  const t = useT();
  const canEdit = useCanEditMetadata();
  const input = useRef<HTMLInputElement>(null);
  const web = isReadOnly();
  return (
    <>
      <button
        type="button"
        className={className}
        style={style}
        onClick={() => (web ? input.current?.click() : void onUpload())}
        {...(web && !canEdit ? { disabled: true, title: t('webReadOnly') } : {})}
      >
        {t('uploadModelImageLabel')}
      </button>
      {web && (
        <input
          ref={input}
          type="file"
          hidden
          data-testid="model-image-input"
          accept=".png,.jpg,.jpeg,.webp,image/png,image/jpeg,image/webp"
          onChange={(event) => {
            const file = event.currentTarget.files?.[0];
            // Picking the same file again must trigger a new change event.
            event.currentTarget.value = '';
            if (file && canEdit) void onUpload(file);
          }}
        />
      )}
    </>
  );
}
