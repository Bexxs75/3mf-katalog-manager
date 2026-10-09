const interactive = 'input, textarea, select, button, a[href], summary, [contenteditable]:not([contenteditable="false"]), ' +
  ['button', 'menuitem', 'menuitemradio', 'menuitemcheckbox', 'option', 'tab', 'switch', 'checkbox', 'radio', 'combobox', 'slider', 'separator', 'tree', 'treeitem', 'listbox', 'spinbutton'].map(role => `[role="${role}"]`).join(', ');

export function shouldIgnoreCatalogShortcut(event: KeyboardEvent, options: {
  allowModifiers?: boolean;
  allowInteractive?: boolean;
  allowMenus?: boolean;
  allowDetailViewToggleArrows?: boolean;
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
  const blocked = (target: EventTarget | null) => {
    if (!(target instanceof Element)) return false;
    const toggle = target.closest('button[data-detail-view-toggle]:not([role])');
    // These plain buttons have no arrow-key semantics; retain their focus for
    // Space/Enter while allowing only detail navigation through the viewer guard.
    if (options.allowDetailViewToggleArrows && /^Arrow(Left|Right)$/.test(event.key) && toggle) {
      for (let node: Element | null = target; node; node = node.parentElement) {
        if (node !== toggle && node.matches(interactive)) return true;
      }
      return false;
    }
    return !!target.closest(interactive + ', [data-detail-viewer]');
  };
  return !options.allowInteractive && (blocked(event.target) || blocked(document.activeElement));
}

export function catalogPlatform(): 'mac' | 'windows' | 'other' {
  const platform = (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform || navigator.platform || navigator.userAgent;
  if (/mac/i.test(platform)) return 'mac';
  if (/win/i.test(platform)) return 'windows';
  return 'other';
}

export function isMacPlatform(): boolean {
  return catalogPlatform() === 'mac';
}

export function hasPlatformModifier(event: KeyboardEvent): boolean {
  return !event.altKey && !event.shiftKey && (isMacPlatform() ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey);
}
