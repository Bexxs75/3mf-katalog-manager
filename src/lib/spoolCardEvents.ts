const INTERACTIVE = 'button, input, textarea, select, a, [role="dialog"]';

/**
 * Doppelklick auf Karte/Zeile oeffnet das Bearbeiten, aber nicht, wenn er auf
 * einem Knopf, Feld oder Popover landet. Ereignisse aus Portalen bubbeln im
 * React-Baum weiter, liegen aber nicht im DOM der Karte: auch die ignorieren.
 */
export function isFromInteractiveElement(event: { target: EventTarget | null; currentTarget: EventTarget | null }): boolean {
  const { target, currentTarget } = event;
  // Auf WebKitGTK/WKWebView kann das dblclick-Ziel ein Text-Node sein (z.B.
  // der Text in einem Knopf oder im Popover) statt des umschliessenden
  // Elements. Auf das Eltern-Element normalisieren, bevor geprueft wird.
  const el = target instanceof Element ? target : target instanceof Node ? target.parentElement : null;
  if (!el) return true;
  if (currentTarget instanceof Node && !currentTarget.contains(el)) return true;
  return el.closest(INTERACTIVE) !== null;
}
