// Phones only: tablets (iPadOS reports "Macintosh", Android tablets omit "Mobile") stay allowed.
const PHONE_UA = /iPhone|iPod|Windows Phone|Android.+Mobile/i;

export function isPhoneBrowser(userAgent: string = navigator.userAgent): boolean {
  return PHONE_UA.test(userAgent);
}

const DISMISS_KEY = 'web.phoneNoticeDismissed';

export function phoneNoticeDismissed(): boolean {
  try { return sessionStorage.getItem(DISMISS_KEY) === '1'; } catch { return false; }
}

export function dismissPhoneNotice(): void {
  try { sessionStorage.setItem(DISMISS_KEY, '1'); } catch { /* The notice then simply returns on reload. */ }
}
