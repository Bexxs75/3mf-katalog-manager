const INTERACTIVE = 'button, input, textarea, select, a, [role="dialog"]';

// On WebKitGTK/WKWebView a mouse event target can be a text node (e.g. the
// text in a button or in the popover) instead of the enclosing element.
function toElement(target: EventTarget | null): Element | null {
  return target instanceof Element ? target : target instanceof Node ? target.parentElement : null;
}

/**
 * A double click on card/row opens editing, but not when it lands on
 * a button, field or popover. Events from portals keep bubbling in the
 * React tree but aren't in the card's DOM: ignore those too.
 */
export function isFromInteractiveElement(event: { target: EventTarget | null; currentTarget: EventTarget | null }): boolean {
  const { currentTarget } = event;
  const el = toElement(event.target);
  if (!el) return true;
  if (currentTarget instanceof Node && !currentTarget.contains(el)) return true;
  return el.closest(INTERACTIVE) !== null;
}

/** No dragging when the mouse down lands on a button of the card/row (edit, delete, confirm). */
export function startsOnButton(event: { target: EventTarget | null }): boolean {
  return toElement(event.target)?.closest('button') != null;
}
