// Keep resolution and the storage key in sync with src/hooks/useTheme.ts.
(function () {
  var setting = 'system';
  try {
    setting = localStorage.getItem('3mf-katalog-theme') || 'system';
  } catch (_) {
    // Storage can be unavailable; the system preference still applies.
  }
  var theme = setting === 'system'
    ? (window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')
    : setting;
  document.documentElement.setAttribute('data-app', theme);
}());
