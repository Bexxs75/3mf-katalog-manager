/**
 * Second barrier before rendering `sourceUrl` as `<a href>` (the backend
 * already filters on write and read): a `javascript:` value would run on
 * click in the app origin with full IPC access.
 */
export function isSafeHttpUrl(url: string | null | undefined): url is string {
  if (!url) return false;
  return url.startsWith('http://') || url.startsWith('https://');
}
