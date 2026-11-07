import { useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { useT } from '../i18n/LanguageContext';
import { settingsHeadingId, type SettingsSectionId } from '../lib/settingsNav';

/**
 * One titled block of the settings panel. The title is the section's `h3` and can take focus, so that a link
 * from elsewhere in the app can land on it. Content that may render nothing (platform dependent switches)
 * hides the whole block with `hideWhenEmpty` rather than leaving a title with nothing below it.
 */
export function SettingsSection({ id, title, experimental, help, danger, hideWhenEmpty, children }: {
  id: SettingsSectionId;
  title: string;
  /** Adds the "Experimental" badge next to the title. */
  experimental?: boolean;
  help?: string;
  /** Sets the block apart for actions that cannot be undone. */
  danger?: boolean;
  hideWhenEmpty?: boolean;
  children?: ReactNode;
}) {
  const t = useT();
  const headingId = settingsHeadingId(id);
  const body = useRef<HTMLDivElement>(null);
  const [empty, setEmpty] = useState(false);
  useLayoutEffect(() => {
    const node = body.current;
    if (!hideWhenEmpty || !node) return;
    const update = () => setEmpty(node.childElementCount === 0 && !node.textContent);
    update();
    const observer = new MutationObserver(update);
    observer.observe(node, { childList: true, subtree: true, characterData: true });
    return () => observer.disconnect();
  }, [hideWhenEmpty]);
  return (
    <section aria-labelledby={headingId} hidden={hideWhenEmpty && empty ? true : undefined}
      className={danger ? 'mt-3 rounded-[6px] bg-[var(--panel-2)] px-3 py-4' : 'py-4 border-b border-[var(--line)] first:pt-1 last:border-b-0'}>
      <h3 id={headingId} tabIndex={-1}
        className={`mb-2 text-[length:var(--font-size-body)] font-semibold outline-none focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)] ${danger ? 'text-[var(--crit)]' : 'text-[var(--ink)]'}`}>
        {title}
        {experimental && <>{' '}<span className="align-middle px-1.5 rounded-full border border-[var(--line-strong)] text-caption font-normal text-[var(--ink-2)]">{t('tvExperimental')}</span></>}
      </h3>
      {help && <p className="-mt-1 mb-2 text-small text-[var(--ink-2)]">{help}</p>}
      <div ref={body}>{children}</div>
    </section>
  );
}
