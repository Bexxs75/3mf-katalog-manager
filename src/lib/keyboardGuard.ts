const interactive = 'input, textarea, select, button, a[href], summary, [contenteditable]:not([contenteditable="false"]), [data-detail-viewer], ' +
  ['button', 'menuitem', 'menuitemradio', 'menuitemcheckbox', 'option', 'tab', 'switch', 'checkbox', 'radio', 'combobox', 'slider', 'separator', 'tree', 'treeitem', 'listbox', 'spinbutton'].map(role => `[role="${role}"]`).join(', ');

export function shouldIgnoreCatalogShortcut(event: KeyboardEvent, options: {
  allowModifiers?: boolean;
  allowInteractive?: boolean;
  allowMenus?: boolean;
  preserveEscapeSelection?: boolean;
} = {}): boolean {
  if (event.defaultPrevented || event.isComposing) return true;
  if (!options.allowModifiers && (event.altKey || event.ctrlKey || event.metaKey ||
    (event.shiftKey && /^(Arrow|Home$|End$|Page|Enter$)/.test(event.key)))) return true;
  if (document.querySelector('[role="dialog"][aria-modal="true"]')) return true;
  if (!options.allowMenus && document.querySelector('[data-navigation-menu], [role="menu"], [role="listbox"], [role="dialog"], .menu')) return true;
  if (options.preserveEscapeSelection && event.key === 'Escape') {
    const textEntry = 'textarea, input:not([type="checkbox"]):not([type="radio"]):not([type="button"]):not([type="submit"]):not([type="reset"]):not([type="range"]):not([type="color"]):not([type="file"]), [contenteditable]:not([contenteditable="false"])';
    return event.target instanceof Element && !!event.target.closest(textEntry);
  }
  const blocked = (target: EventTarget | null) => target instanceof Element && !!target.closest(interactive);
  return !options.allowInteractive && (blocked(event.target) || blocked(document.activeElement));
}

export function isMacPlatform(): boolean {
  const platform = (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform || navigator.platform || navigator.userAgent;
  return /mac/i.test(platform);
}

export function hasPlatformModifier(event: KeyboardEvent): boolean {
  return !event.altKey && !event.shiftKey && (isMacPlatform() ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey);
}
