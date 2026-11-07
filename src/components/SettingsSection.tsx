import { createContext, useCallback, useContext, useId, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { useT } from '../i18n/LanguageContext';
import { settingsHeadingId, type SettingsSectionId } from '../lib/settingsNav';

type ReportContent = (reporter: string, hasContent: boolean) => void;
const SectionContent = createContext<ReportContent | null>(null);

/**
 * Lets a component inside a `hideWhenEmpty` section say whether it shows anything right now. A control that
 * can turn out to be unsupported on this system (start menu entry, close behavior) reports `false` then, and
 * the section drops its title instead of leaving it with nothing below. Outside such a section it does nothing.
 */
export function useSectionContent(hasContent: boolean) {
  const report = useContext(SectionContent);
  const reporter = useId();
  useLayoutEffect(() => {
    report?.(reporter, hasContent);
    return () => report?.(reporter, false);
  }, [report, reporter, hasContent]);
}

/**
 * One titled block of the settings panel. The title is the section's `h3` and can take focus, so that a link
 * from elsewhere in the app can land on it. Content that may render nothing (platform dependent switches)
 * hides the whole block with `hideWhenEmpty` (its children report through `useSectionContent`) rather than leaving a title with nothing below it.
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
  // Until its children have reported, a section that may hide counts as filled, so nothing flickers away at mount.
  const [empty, setEmpty] = useState(false);
  const reporters = useRef(new Set<string>());
  const report = useCallback<ReportContent>((reporter, hasContent) => {
    if (hasContent) reporters.current.add(reporter); else reporters.current.delete(reporter);
    setEmpty(reporters.current.size === 0);
  }, []);
  // The children's effects have run before this one: a section nobody reported into has nothing to show.
  useLayoutEffect(() => { if (hideWhenEmpty) setEmpty(reporters.current.size === 0); }, [hideWhenEmpty]);
  const reportContent = useMemo(() => (hideWhenEmpty ? report : null), [hideWhenEmpty, report]);
  return (
    <section aria-labelledby={headingId} hidden={hideWhenEmpty && empty ? true : undefined}
      className={danger ? 'mt-3 rounded-[6px] bg-[var(--panel-2)] px-3 py-4' : 'py-4 border-b border-[var(--line)] first:pt-1 last:border-b-0'}>
      <h3 id={headingId} tabIndex={-1}
        className={`mb-2 text-[length:var(--font-size-body)] font-semibold outline-none focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)] ${danger ? 'text-[var(--crit)]' : 'text-[var(--ink)]'}`}>
        {title}
        {experimental && <>{' '}<span className="align-middle px-1.5 rounded-full border border-[var(--line-strong)] text-caption font-normal text-[var(--ink-2)]">{t('tvExperimental')}</span></>}
      </h3>
      {help && <p className="-mt-1 mb-2 text-small text-[var(--ink-2)]">{help}</p>}
      <SectionContent.Provider value={reportContent}>{children}</SectionContent.Provider>
    </section>
  );
}
