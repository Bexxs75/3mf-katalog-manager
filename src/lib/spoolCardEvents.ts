const INTERACTIVE = 'button, input, textarea, select, a, [role="dialog"]';

/**
 * A double click on card/row opens editing, but not when it lands on
 * a button, field or popover. Events from portals keep bubbling in the
 * React tree but aren't in the card's DOM: ignore those too.
 */
export function isFromInteractiveElement(event: { target: EventTarget | null; currentTarget: EventTarget | null }): boolean {
  const { target, currentTarget } = event;
  // On WebKitGTK/WKWebView the dblclick target can be a text node (e.g.
  // the text in a button or in the popover) instead of the enclosing
  // element. Normalize to the parent element before checking.
  const el = target instanceof Element ? target : target instanceof Node ? target.parentElement : null;
  if (!el) return true;
  if (currentTarget instanceof Node && !currentTarget.contains(el)) return true;
  return el.closest(INTERACTIVE) !== null;
}
