/**
 * The webview's built-in right-click menu offers browser actions ("Reload",
 * "Back", "Print", ...) that make no sense in a desktop app - "Reload" even
 * looks like "refresh the catalog" but only reloads the interface. It stays
 * where it is useful: in text fields (cut/copy/paste) and on selected text
 * (copy). Development builds keep it for "Inspect". The app's own context
 * menus are unaffected; they handle the event themselves.
 */
export function installContextMenuGuard(doc: Document, isDev: boolean): () => void {
  if (isDev) return () => {};
  const onContextMenu = (event: MouseEvent) => {
    const target = event.target;
    if (target instanceof Element && target.closest('input, textarea, [contenteditable]:not([contenteditable="false"])')) {
      return;
    }
    const selection = doc.getSelection();
    if (selection && !selection.isCollapsed && selection.toString().trim() !== '') return;
    event.preventDefault();
  };
  doc.addEventListener('contextmenu', onContextMenu);
  return () => doc.removeEventListener('contextmenu', onContextMenu);
}
