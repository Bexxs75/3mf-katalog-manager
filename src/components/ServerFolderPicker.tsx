import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent, type RefObject } from 'react';
import { useT, useFormatCount } from '../i18n/LanguageContext';
import { ErrorText } from '../diagnostics/ErrorText';
import { toAppError, type AppError } from '../lib/errors';
import { displayName } from '../lib/displayName';
import { listSourceDirs } from '../lib/api/sourceFolders';
import type { SourceDirRoot } from '../types';
import { CatalogActionDialog, catalogActionBase, catalogActionButton } from './CatalogActionDialog';

const primary = `${catalogActionBase} border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)]`;
const focusRing = 'focus-visible:outline-2 focus-visible:outline-[var(--accent)]';

interface TreeNode {
  key: string;
  domId: string;
  rootIdx: number;
  /** Path below the root; empty for the root itself. */
  rel: string;
  name: string;
  /** Supported files inside, subfolders included; null until the parent listing delivered it. */
  count: number | null;
  /** The count is a lower bound because the server's entry limit was reached. */
  approx: boolean;
}
interface Branch {
  status: 'loading' | 'loaded' | 'error';
  children: string[];
  /** The server stopped looking at its limit, so the list and the counts may be incomplete. */
  truncated: boolean;
}
interface Row { node: TreeNode; level: number; size: number; pos: number }

const keyOf = (rootIdx: number, rel: string) => `${rootIdx}:${rel}`;
const iconProps = { fill: 'none', stroke: 'currentColor', 'aria-hidden': true } as const;

export interface ServerFolderPickerProps {
  onClose: () => void;
  returnFocus?: RefObject<HTMLElement | null>;
  /** Stores the chosen folder as a source folder; rejects with the backend error. */
  onRemember?: (rootIdx: number, rel: string) => Promise<unknown>;
  /** Copies the chosen folder into the catalog. Omitted while the server cannot do that. */
  onImport?: (rootIdx: number, rel: string) => Promise<unknown>;
}

/** Read-only browser for the source roots the server shares; it only ever deals in root indexes and relative paths. */
export function ServerFolderPicker({ onClose, returnFocus, onRemember, onImport }: ServerFolderPickerProps) {
  const t = useT();
  const count = useFormatCount();
  const [roots, setRoots] = useState<{ status: 'loading' | 'ready' | 'error'; error?: AppError }>({ status: 'loading' });
  const [nodes, setNodes] = useState<Record<string, TreeNode>>({});
  const [rootKeys, setRootKeys] = useState<string[]>([]);
  const [branches, setBranches] = useState<Record<string, Branch>>({});
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const [selected, setSelected] = useState<string | null>(null);
  const [active, setActive] = useState<string | null>(null);
  const [treeError, setTreeError] = useState<{ key: string | null; error: AppError } | null>(null);
  const [rememberedKey, setRememberedKey] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<AppError | null>(null);
  const [liveNote, setLiveNote] = useState('');
  const mounted = useRef(true);
  const treeRef = useRef<HTMLDivElement>(null);
  const fallbackRef = useRef<HTMLButtonElement>(null);
  const idCounter = useRef(0);
  const nodesRef = useRef(nodes);
  nodesRef.current = nodes;
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  const makeNode = (rootIdx: number, rel: string, name: string, count: number | null, approx: boolean): TreeNode =>
    ({ key: keyOf(rootIdx, rel), domId: `sfp-item-${idCounter.current++}`, rootIdx, rel, name, count, approx });

  const loadBranch = useCallback(async (key: string) => {
    const node = nodesRef.current[key];
    if (!node) return;
    setBranches(prev => ({ ...prev, [key]: { status: 'loading', children: [], truncated: false } }));
    setLiveNote(t('sfpLoadingChildren'));
    try {
      const listing = await listSourceDirs(node.rootIdx, node.rel);
      if (!mounted.current) return;
      const children = listing.entries.filter(entry => entry.directory)
        .map(entry => makeNode(node.rootIdx, entry.rel, entry.name, entry.fileCount, entry.truncated));
      // A root has no count of its own; its entries (folders and loose files) add up to it.
      const rootCount = listing.entries.reduce((sum, entry) => sum + entry.fileCount, 0);
      setNodes(prev => {
        const next = { ...prev };
        for (const child of children) next[child.key] = child;
        if (node.rel === '') next[key] = { ...next[key], count: rootCount, approx: listing.truncated };
        return next;
      });
      setBranches(prev => ({ ...prev, [key]: { status: 'loaded', children: children.map(c => c.key), truncated: listing.truncated } }));
      setTreeError(prev => (prev?.key === key ? null : prev));
      setLiveNote(t('sfpChildrenLoaded'));
    } catch (e) {
      if (!mounted.current) return;
      setBranches(prev => ({ ...prev, [key]: { status: 'error', children: [], truncated: false } }));
      setTreeError({ key, error: toAppError(e) });
      setLiveNote('');
    }
  // makeNode only uses a ref counter, so it is stable enough to leave out.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [t]);

  const loadRoots = useCallback(async () => {
    setRoots({ status: 'loading' });
    try {
      const listing = await listSourceDirs();
      if (!mounted.current) return;
      const list: SourceDirRoot[] = listing.roots;
      const created = list.map(root => makeNode(root.rootIdx, '', root.name, null, false));
      setNodes(Object.fromEntries(created.map(node => [node.key, node])));
      nodesRef.current = Object.fromEntries(created.map(node => [node.key, node]));
      setRootKeys(created.map(node => node.key));
      setBranches({}); setExpanded(new Set());
      setActive(created[0]?.key ?? null);
      setRoots({ status: 'ready' });
      // With one root there is nothing to choose between: show its content right away.
      if (created.length === 1) { setExpanded(new Set([created[0].key])); void loadBranch(created[0].key); }
    } catch (e) {
      if (mounted.current) setRoots({ status: 'error', error: toAppError(e) });
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loadBranch]);
  useEffect(() => { void loadRoots(); }, [loadRoots]);

  // Without roots or after a failure the tree is gone; hand focus to a control that is still there.
  useEffect(() => {
    if (roots.status === 'error' || (roots.status === 'ready' && rootKeys.length === 0)) fallbackRef.current?.focus();
  }, [roots.status, rootKeys.length]);

  const rows = useMemo<Row[]>(() => {
    const out: Row[] = [];
    const walk = (keys: string[], level: number) => keys.forEach((key, i) => {
      const node = nodes[key];
      if (!node) return;
      out.push({ node, level, size: keys.length, pos: i + 1 });
      const branch = branches[key];
      if (expanded.has(key) && branch?.status === 'loaded') walk(branch.children, level + 1);
    });
    walk(rootKeys, 1);
    return out;
  }, [nodes, rootKeys, branches, expanded]);

  const isLeaf = (key: string) => { const b = branches[key]; return b?.status === 'loaded' && b.children.length === 0; };
  const parentOf = (key: string) => {
    const node = nodes[key];
    if (!node || node.rel === '') return null;
    const cut = node.rel.lastIndexOf('/');
    return keyOf(node.rootIdx, cut < 0 ? '' : node.rel.slice(0, cut));
  };

  const open = (key: string) => {
    if (isLeaf(key)) return;
    setExpanded(prev => new Set(prev).add(key));
    const status = branches[key]?.status;
    if (!status || status === 'error') void loadBranch(key);
  };
  const close = (key: string) => setExpanded(prev => { const next = new Set(prev); next.delete(key); return next; });
  const choose = (key: string) => {
    setSelected(key); setActive(key); setRememberedKey(null); setActionError(null);
    // A failed folder blocks the actions only until the user picks something else.
    setTreeError(null);
  };

  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.altKey || event.ctrlKey || event.metaKey || !active) return;
    const index = rows.findIndex(row => row.node.key === active);
    let handled = true;
    switch (event.key) {
      case 'ArrowDown': setActive(rows[Math.min(rows.length - 1, index + 1)]?.node.key ?? active); break;
      case 'ArrowUp': setActive(rows[Math.max(0, index - 1)]?.node.key ?? active); break;
      case 'Home': setActive(rows[0]?.node.key ?? active); break;
      case 'End': setActive(rows[rows.length - 1]?.node.key ?? active); break;
      case 'ArrowRight':
        if (isLeaf(active)) break;
        if (!expanded.has(active)) open(active);
        else if (branches[active]?.children[0]) setActive(branches[active].children[0]);
        break;
      case 'ArrowLeft':
        if (expanded.has(active)) close(active);
        else { const parent = parentOf(active); if (parent) setActive(parent); }
        break;
      case 'Enter': case ' ': choose(active); break;
      default: handled = false;
    }
    if (handled) event.preventDefault();
  };

  const activeDomId = active ? nodes[active]?.domId : undefined;
  useEffect(() => {
    const element = activeDomId ? document.getElementById(activeDomId) : null;
    element?.scrollIntoView?.({ block: 'nearest' });
  }, [activeDomId]);

  const chosen = selected ? nodes[selected] : null;
  const crumbs = useMemo(() => {
    if (!chosen) return [];
    const rootName = nodes[keyOf(chosen.rootIdx, '')]?.name ?? '';
    const parts = chosen.rel === '' ? [] : chosen.rel.split('/');
    return [{ key: keyOf(chosen.rootIdx, ''), name: rootName },
      ...parts.map((name, i) => ({ key: keyOf(chosen.rootIdx, parts.slice(0, i + 1).join('/')), name }))];
  }, [chosen, nodes]);

  const truncatedBranch = Object.entries(branches).find(([key, b]) => b.truncated && expanded.has(key));
  const blocked = treeError !== null || busy || !chosen;

  const act = async (action: (rootIdx: number, rel: string) => Promise<unknown>, done: (key: string) => void) => {
    if (!chosen || blocked) return;
    setBusy(true); setActionError(null);
    try { await action(chosen.rootIdx, chosen.rel); if (mounted.current) done(chosen.key); }
    catch (e) { if (mounted.current) setActionError(toAppError(e)); }
    finally { if (mounted.current) setBusy(false); }
  };
  const remember = () => act(onRemember!, key => { setRememberedKey(key); });
  const doImport = () => act(onImport!, () => onClose());
  // The pressed button turns disabled; keep focus on the tree, which stays usable.
  useEffect(() => { if (!busy && rememberedKey) treeRef.current?.focus(); }, [busy, rememberedKey]);

  const retry = () => {
    const failed = treeError?.key;
    setTreeError(null);
    if (failed) void loadBranch(failed); else void loadRoots();
  };
  const fileText = (node: TreeNode) => count(t(node.approx ? 'sfpFilesAtLeast' : 'sfpFiles'), node.count ?? 0);
  const empty = roots.status === 'ready' && rootKeys.length === 0;

  return (
    <CatalogActionDialog title={t('sfpTitle')} onClose={onClose} busy={busy} wide returnFocus={returnFocus} closeLabel={t('sfpClose')}>
      {onImport && (
        <div className="rounded border border-[var(--line)] bg-[var(--panel-2)] px-3 py-2 text-small">
          {t('sfpCopyNote')}
        </div>
      )}
      {empty && (
        <>
          <div role="status" className="rounded border border-[var(--line-strong)] bg-[var(--panel-2)] px-3 py-2 text-small">
            <b className="font-semibold">{t('sfpNoRoots')}</b> {t('sfpNoRootsHint')}
          </div>
          <p className="text-small text-[var(--ink-2)]">{t('sfpNoRootsUpload')}</p>
          <div className="flex justify-end"><button ref={fallbackRef} type="button" className={`${catalogActionButton} ${focusRing}`} onClick={onClose}>{t('sfpClose')}</button></div>
        </>
      )}
      {roots.status === 'error' && (
        <>
          <p role="alert" className="text-small text-[var(--crit)] whitespace-pre-wrap [overflow-wrap:anywhere]"><ErrorText error={roots.error ?? null} /></p>
          <div className="flex justify-end gap-2">
            <button type="button" className={`${catalogActionButton} ${focusRing}`} onClick={onClose}>{t('sfpClose')}</button>
            <button ref={fallbackRef} type="button" className={`${primary} ${focusRing}`} onClick={() => void loadRoots()}>{t('sfpReload')}</button>
          </div>
        </>
      )}
      {(roots.status === 'loading' || (roots.status === 'ready' && !empty)) && (
        <>
          {crumbs.length > 0 && (
            <nav aria-label={t('sfpCrumbLabel')}>
              <ol className="flex flex-wrap items-center gap-x-1 text-small">
                {crumbs.map((crumb, i) => (
                  <li key={crumb.key} className="flex items-center gap-1">
                    {i === crumbs.length - 1
                      ? <span aria-current="location" className="font-semibold">{displayName(crumb.name)}</span>
                      : <button type="button" className={`text-[var(--accent)] underline underline-offset-2 cursor-pointer rounded-sm ${focusRing}`}
                        onClick={() => { choose(crumb.key); treeRef.current?.focus(); }}>{displayName(crumb.name)}</button>}
                    {i < crumbs.length - 1 && <span aria-hidden="true" className="text-[var(--ink-3)]">›</span>}
                  </li>
                ))}
              </ol>
            </nav>
          )}
          <div ref={treeRef} role="tree" tabIndex={0} aria-label={t('sfpTreeLabel')} aria-busy={roots.status === 'loading'}
            aria-activedescendant={activeDomId} aria-describedby={treeError ? 'sfp-error' : undefined} data-initial-focus
            onKeyDown={keyDown}
            className={`max-h-[260px] overflow-auto rounded-md border border-[var(--line)] p-1 ${focusRing}`}>
            {roots.status === 'loading' && <p className="px-2 py-1.5 text-small italic text-[var(--ink-3)]">{t('sfpLoadingRoots')}</p>}
            {rows.map(({ node, level, size, pos }) => {
              const expandable = !isLeaf(node.key);
              const isOpen = expanded.has(node.key);
              const isSel = selected === node.key;
              const loading = isOpen && branches[node.key]?.status === 'loading';
              return (
                <div key={node.key} id={node.domId} role="treeitem" aria-level={level} aria-setsize={size} aria-posinset={pos}
                  aria-expanded={expandable ? isOpen : undefined} aria-selected={isSel} aria-busy={loading || undefined}
                  onClick={() => choose(node.key)}
                  style={{ paddingLeft: 8 + (level - 1) * 16 }}
                  className={`flex h-[30px] cursor-pointer select-none items-center gap-1.5 rounded-[5px] pr-2 text-small ${isSel ? 'bg-[var(--accent-soft)] font-semibold text-[var(--accent)]' : 'text-[var(--ink-2)] hover:bg-[var(--panel-2)]'} ${active === node.key ? 'outline outline-2 -outline-offset-2 outline-[var(--accent)] [[role=tree]:not(:focus)_&]:outline-none' : ''}`}>
                  <span aria-hidden="true" className="inline-flex h-3.5 w-3.5 shrink-0 items-center justify-center"
                    onClick={event => { if (!expandable) return; event.stopPropagation(); setActive(node.key); if (isOpen) close(node.key); else open(node.key); }}>
                    {expandable && <svg width="10" height="10" viewBox="0 0 10 10" strokeWidth="1.6" {...iconProps} style={{ transform: isOpen ? 'rotate(90deg)' : undefined }}><path d="M3 1.5l4 3.5-4 3.5" /></svg>}
                  </span>
                  <svg width="14" height="14" viewBox="0 0 16 16" strokeWidth="1.4" className="shrink-0" {...iconProps}><path d="M1.5 4.5v8h13v-7H7.5l-1.5-2h-4.5z" /></svg>
                  <span className="min-w-0 flex-1 truncate">{displayName(node.name)}</span>
                  {loading
                    ? <span className="text-caption font-normal italic text-[var(--ink-3)]">{t('sfpRowLoading')}</span>
                    : node.count !== null && (
                      <span className={`text-caption font-medium tabular-nums ${isSel ? '' : 'text-[var(--ink-3)]'}`}>
                        <span className="sr-only">{fileText(node)}</span>
                        <span aria-hidden="true">{node.count}{node.approx ? '+' : ''}</span>
                      </span>)}
                </div>
              );
            })}
          </div>
          <p role="status" aria-live="polite" className="sr-only">{liveNote}</p>
          {truncatedBranch && nodes[truncatedBranch[0]] && (
            <div role="status" className="rounded border border-[var(--warn)] bg-[var(--warn-soft)] px-3 py-2 text-small">
              <b className="font-semibold">{t('sfpTruncatedTitle')}</b> {t('sfpTruncated').replace('{name}', displayName(nodes[truncatedBranch[0]].name))}
            </div>
          )}
          {treeError ? (
            <div id="sfp-error" role="alert" className="rounded border border-[var(--crit)] bg-[var(--crit-soft)] px-3 py-2 text-small">
              <p className="whitespace-pre-wrap [overflow-wrap:anywhere]"><ErrorText error={treeError.error} /></p>
              <button type="button" className={`${catalogActionButton} ${focusRing} mt-2`} onClick={retry}>{t('sfpReload')}</button>
            </div>
          ) : (
            <div aria-live="polite" className="rounded border border-[var(--line-strong)] bg-[var(--panel-2)] px-3 py-2 text-small">
              {!chosen ? t('sfpSummaryNone')
                : <>
                  <b className="font-semibold [overflow-wrap:anywhere]">{displayName(chosen.name)}</b><br />
                  {chosen.count === null ? t('sfpSummaryUnknown')
                    : count(t(chosen.approx ? 'sfpSummaryAtLeast' : 'sfpSummary'), chosen.count)}
                  {onImport && <> {t('sfpSummaryCopy')}</>}
                </>}
            </div>
          )}
          <p role="status" className="text-small text-[var(--ink-2)] empty:hidden">
            {rememberedKey && nodes[rememberedKey] ? t('sfpRememberedNote').replace('{name}', displayName(nodes[rememberedKey].name)) : ''}
          </p>
          {actionError && <p role="alert" className="text-small text-[var(--crit)] whitespace-pre-wrap [overflow-wrap:anywhere]"><ErrorText error={actionError} /></p>}
          <div className="flex flex-wrap justify-end gap-2">
            <button type="button" className={`${catalogActionButton} ${focusRing}`} disabled={busy} onClick={onClose}>{t('cancel')}</button>
            {onRemember && (
              <button type="button" className={`${onImport ? catalogActionButton : primary} ${focusRing}`} disabled={blocked || rememberedKey === selected} onClick={() => void remember()}>
                {rememberedKey !== null && rememberedKey === selected ? t('sfpRemembered') : t('sfpRemember')}
              </button>
            )}
            {onImport && (
              <button type="button" className={`${primary} ${focusRing}`} disabled={blocked} onClick={() => void doImport()}>
                {busy ? t('sfWorking') : t('sfpImport')}
              </button>
            )}
          </div>
        </>
      )}
    </CatalogActionDialog>
  );
}
