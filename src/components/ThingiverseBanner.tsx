import { useT } from '../i18n/LanguageContext';
import { ErrorText } from '../diagnostics/ErrorText';
import { useFormatError } from '../diagnostics/useFormatError';
import type { AppError } from '../lib/errors';

type TitleKey = 'tvTitleOffline' | 'tvTitleInvalid' | 'tvTitleBlocked' | 'tvTitleRate' | 'tvTitleDaily' | 'tvTitleQuery' | 'tvTitleBad' | 'tvTitleNotFound' | 'tvTitleNotSetUp';

const BANNER: Record<string, { title: TitleKey; tone: 'error' | 'warn' | 'info'; action?: 'retry' | 'settings' | 'back' }> = {
  thingiverseOffline: { title: 'tvTitleOffline', tone: 'error', action: 'retry' },
  thingiverseTimeout: { title: 'tvTitleOffline', tone: 'error', action: 'retry' },
  thingiverseTokenInvalid: { title: 'tvTitleInvalid', tone: 'error', action: 'settings' },
  thingiverseBlocked: { title: 'tvTitleBlocked', tone: 'warn', action: 'retry' },
  thingiverseRateLimited: { title: 'tvTitleRate', tone: 'warn' },
  thingiverseDailyLimit: { title: 'tvTitleDaily', tone: 'warn' },
  thingiverseQueryInvalid: { title: 'tvTitleQuery', tone: 'info' },
  thingiverseBadResponse: { title: 'tvTitleBad', tone: 'error', action: 'retry' },
  thingiverseNotFound: { title: 'tvTitleNotFound', tone: 'info', action: 'back' },
  thingiverseDisabled: { title: 'tvTitleNotSetUp', tone: 'info', action: 'settings' },
  thingiverseNoToken: { title: 'tvTitleNotSetUp', tone: 'info', action: 'settings' },
};

const buttonClass = 'h-7 px-2 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)]';

/**
 * One banner per error code of the Thingiverse area: a title, the cause and what to do
 * about it. Not a dialog: the results already loaded stay visible underneath.
 */
export function ThingiverseBanner({ error, onRetry, onSettings, onBack }: {
  error: AppError;
  onRetry?: () => void;
  onSettings: () => void;
  onBack?: () => void;
}) {
  const t = useT();
  const format = useFormatError();
  const spec = BANNER[error.message];
  if (!spec) return <div role="alert" className="mb-3 p-3 rounded border border-[var(--crit)] text-[var(--crit)] break-words"><ErrorText error={error} /></div>;
  const tone = spec.tone === 'warn' ? 'border-[var(--warn)] bg-[var(--warn-soft)]'
    : spec.tone === 'info' ? 'border-[var(--line-strong)] bg-[var(--panel-2)]' : 'border-[var(--crit)] bg-[var(--crit-soft)]';
  const action = spec.action === 'retry' && onRetry ? <button type="button" className={buttonClass} onClick={onRetry}>{t('tvRetry')}</button>
    : spec.action === 'settings' ? <button type="button" className={buttonClass} onClick={onSettings}>{t('tvOpenSettings')}</button>
    : spec.action === 'back' && onBack ? <button type="button" className={buttonClass} onClick={onBack}>{t('tvBackToList')}</button> : null;
  return (
    <div role={spec.tone === 'info' ? 'status' : 'alert'} className={`mb-3 p-3 rounded border flex flex-wrap items-center justify-between gap-2 ${tone}`}>
      <div className="min-w-0">
        <b className="block">{t(spec.title)}</b>
        <span className="text-small text-[var(--ink-2)] break-words">{format(error)}</span>
      </div>
      {action}
    </div>
  );
}
