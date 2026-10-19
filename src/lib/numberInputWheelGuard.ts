/**
 * Chromium-based webviews change the value of a focused number field when the
 * mouse wheel turns over it, so scrolling the page silently edits the number.
 * Taking focus away from the field on wheel makes the browser leave the value
 * alone and the page keeps scrolling; the arrow keys and spin buttons work
 * again after the next click or Tab. One listener on the document covers every
 * number field, including ones added later.
 */
export function installNumberInputWheelGuard(doc: Document): () => void {
  const onWheel = (event: WheelEvent) => {
    const target = event.target;
    if (target instanceof HTMLInputElement && target.type === 'number' && doc.activeElement === target) {
      target.blur();
    }
  };
  // Passive: the event itself must stay untouched so the page scrolls as usual.
  doc.addEventListener('wheel', onWheel, { capture: true, passive: true });
  return () => doc.removeEventListener('wheel', onWheel, { capture: true });
}
