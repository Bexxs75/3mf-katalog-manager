import { useEffect, useId, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { useT } from '../i18n/LanguageContext';
import { Icon } from './Icon';
import { ViewportPopover } from './ViewportPopover';

interface Props {
  tag: string;
  /** True when the app would assign this tag by itself: the remove button then opens a menu. */
  autoTag: boolean;
  onRemove: () => void;
  onRemoveAndBlock: () => void;
  className?: string;
  style?: CSSProperties;
  /** Keeps the look of the plain button where the chip is not accent coloured. */
  menuAnchorClassName?: string;
}

/**
 * The remove button of a tag chip. Plain tags are removed with one click. An automatic tag opens
 * a small menu with a second choice: remove it and never assign it automatically again.
 */
export function TagRemoveButton({ tag, autoTag, onRemove, onRemoveAndBlock, className, style, menuAnchorClassName }: Props) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const wrapperRef = useRef<HTMLSpanElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const hintId = useId();

  // The menu closes when another model is shown or the tag stops being an automatic one.
  useEffect(() => { setOpen(false); }, [tag, autoTag]);

  useEffect(() => {
    if (!open) return;
    menuRef.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
    const outside = (event: MouseEvent) => {
      if (wrapperRef.current && !wrapperRef.current.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', outside);
    return () => document.removeEventListener('mousedown', outside);
  }, [open]);

  const removeLabel = t('chipRemove').replace('{label}', tag);
  if (!autoTag) {
    return (
      <button type="button" data-tag-remove aria-label={removeLabel} onClick={onRemove} className={className} style={style}>
        <Icon name="close" size={14} />
      </button>
    );
  }

  const close = (focusButton: boolean) => {
    setOpen(false);
    if (focusButton) buttonRef.current?.focus();
  };

  // After a plain removal the chip is gone and takes its button along: hand focus to the
  // neighbouring control of the tag list instead of dropping it on the page.
  const removeOnly = () => {
    const list = wrapperRef.current?.closest<HTMLElement>('[data-tag-list]') ?? null;
    setOpen(false);
    onRemove();
    window.setTimeout(() => {
      list?.querySelector<HTMLElement>('[data-tag-remove], input')?.focus();
    }, 0);
  };

  // The status message with the undo button takes focus once the block went through; until
  // then (and if it fails) focus stays on the button.
  const removeAndBlock = () => {
    close(true);
    onRemoveAndBlock();
  };

  const onMenuKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      close(true);
      return;
    }
    if (event.key === 'Tab') { setOpen(false); return; }
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    const items = Array.from(event.currentTarget.querySelectorAll<HTMLElement>('[role="menuitem"]'));
    const current = items.indexOf(document.activeElement as HTMLElement);
    if (current < 0) return;
    event.preventDefault();
    items[(current + (event.key === 'ArrowDown' ? 1 : items.length - 1)) % items.length].focus();
  };

  return (
    <span ref={wrapperRef} className={`relative flex-none inline-flex ${menuAnchorClassName ?? ''}`}>
      <button
        ref={buttonRef}
        type="button"
        data-tag-remove
        aria-label={t('chipRemoveWithMenu').replace('{label}', tag)}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        onKeyDown={(event) => { if (event.key === 'ArrowDown' && !open) { event.preventDefault(); setOpen(true); } }}
        className={className}
        style={style}
      >
        <Icon name="close" size={14} />
      </button>
      {open && (
        <ViewportPopover
          data-navigation-menu
          className="absolute top-full right-0 mt-1 z-40 w-[250px] max-w-[calc(100vw-16px)] py-1 bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] text-left font-normal text-[var(--ink)]"
        >
          <div ref={menuRef} role="menu" aria-label={t('autoTagMenuAria').replace('{label}', tag)} onKeyDown={onMenuKeyDown}>
            <button
              type="button" role="menuitem" title={t('autoTagRemoveOnlyHint')} aria-describedby={`${hintId}-plain`}
              onClick={removeOnly}
              className="w-full text-left px-3 py-1.5 text-small cursor-pointer hover:bg-[var(--panel-2)] focus-visible:bg-[var(--panel-2)]"
            >
              {t('autoTagRemoveOnly')}
            </button>
            <button
              type="button" role="menuitem" title={t('autoTagRemoveAndBlockHint')} aria-describedby={`${hintId}-block`}
              onClick={removeAndBlock}
              className="w-full text-left px-3 py-1.5 text-small cursor-pointer hover:bg-[var(--panel-2)] focus-visible:bg-[var(--panel-2)]"
            >
              {t('autoTagRemoveAndBlock')}
            </button>
            <span id={`${hintId}-plain`} className="sr-only">{t('autoTagRemoveOnlyHint')}</span>
            <span id={`${hintId}-block`} className="sr-only">{t('autoTagRemoveAndBlockHint')}</span>
          </div>
        </ViewportPopover>
      )}
    </span>
  );
}
