import { useModelImages } from '../hooks/useModelImages';
import { useT } from '../i18n/LanguageContext';
import { displayName } from '../lib/displayName';
import { isResin, kindBadge } from '../lib/fileKind';
import { resolveDisplayImage } from '../lib/resolveDisplayImage';
import type { ModelFile } from '../types';

/**
 * Stands where the 3D viewer is for a model. Sliced output holds tool paths or layer images, not a
 * model shape, so there is nothing to draw: the picture the slicer embedded (or one the user added)
 * is shown when there is one, else a calm card says why there is no preview.
 */
export function SlicedPreview({ model, surfaceClassName = '' }: {
  model: Pick<ModelFile, 'id' | 'name' | 'path' | 'fileType' | 'customImage' | 'thumbnailImage' | 'renderSnapshotImage'>;
  surfaceClassName?: string;
}) {
  const t = useT();
  const resin = isResin(model);
  const images = useModelImages([model.id]);
  const image = resolveDisplayImage(model, 'thumbnail', images.get(model.id));
  const badge = (
    <span data-kind-badge aria-hidden="true"
      className="absolute left-2.5 top-2.5 z-[1] font-bold tracking-[0.05em] text-caption px-1.5 py-px rounded border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)]">
      {kindBadge(model)}
    </span>
  );
  return (
    <div data-sliced-preview className={`relative w-full ${surfaceClassName}`}>
      {image ? (
        <>
          <img src={image} alt={displayName(model.name)} className="absolute inset-0 w-full h-full object-contain" />
          {badge}
        </>
      ) : (
        <>
          {badge}
          <div className="absolute inset-0 grid place-items-center p-4">
            <div role="note" className="max-w-[300px] rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-4 py-3.5 text-center">
              <h3 className="m-0 mb-1 text-body font-semibold text-[var(--ink)]">{t(resin ? 'resinNoPreviewTitle' : 'gcodeNoPreviewTitle')}</h3>
              <p className="m-0 text-small text-[var(--ink-2)]">{t(resin ? 'resinNoPreviewBody' : 'gcodeNoPreviewBody')}</p>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
