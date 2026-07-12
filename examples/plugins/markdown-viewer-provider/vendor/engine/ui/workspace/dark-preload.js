// Synchronously apply the cached dark-mode class before the page stylesheet
// parses. Loaded via <script src> (CSP for MV3 extension_pages does not allow
// inline script hashes, so we ship this as a tiny separate file).
// The flag is written by theme-to-css.ts (inside the iframe) and by the
// workspace init code (outer page), so both surfaces converge quickly.
(function () {
  try {
    if (sessionStorage.getItem('mdv-dark') === '1') {
      document.documentElement.classList.add('dark');
    }
  } catch (_) { /* storage disabled */ }
})();
