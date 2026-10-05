// Runs INSIDE the app's webview (injected as text by lib/audit.ts), so it must
// be plain, self-contained JavaScript: no imports, no TypeScript, no helpers
// from the test process.
function __mfkAudit(kind, opts) {
  var allow = opts.allow || {};

  function describe(el) {
    var cls = typeof el.className === 'string' ? el.className.trim().split(/\s+/).slice(0, 3).join('.') : '';
    return el.tagName.toLowerCase() + (cls ? '.' + cls : '');
  }
  function matchesAny(el, list) {
    for (var i = 0; i < (list || []).length; i++) {
      try { if (el.matches(list[i].selector) || el.closest(list[i].selector)) return true; } catch (e) { /* bad selector is reported by the harness */ }
    }
    return false;
  }
  function isVisible(el) {
    var r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return false;
    // The hidden snapshot renderer lives far outside the viewport.
    if (r.right < 0 || r.bottom < 0 || r.left > innerWidth || r.top > innerHeight) return false;
    for (var n = el; n && n.nodeType === 1; n = n.parentElement) {
      var cs = getComputedStyle(n);
      if (cs.display === 'none' || cs.visibility === 'hidden' || cs.opacity === '0') return false;
    }
    return true;
  }
  function ownText(el) {
    var t = '';
    for (var c = el.firstChild; c; c = c.nextSibling) if (c.nodeType === 3) t += c.nodeValue;
    return t.replace(/\s+/g, ' ').trim();
  }
  function rectOf(el) {
    var r = el.getBoundingClientRect();
    return { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height) };
  }

  var problems = [];
  var all = document.querySelectorAll('body *');

  if (kind === 'layout') {
    // (a) truncated text
    for (var i = 0; i < all.length; i++) {
      var el = all[i];
      var text = ownText(el);
      if (!text || !isVisible(el)) continue;
      var cs = getComputedStyle(el);
      var truncated = el.scrollWidth > el.clientWidth + 1;
      if (!truncated) continue;
      if (cs.textOverflow === 'ellipsis' && matchesAny(el, allow.ellipsis)) continue;
      if (cs.textOverflow !== 'ellipsis') {
        // Text clipped without an ellipsis: only a problem if the element really clips (overflow hidden/clip).
        if (cs.overflowX !== 'hidden' && cs.overflowX !== 'clip') continue;
        if (matchesAny(el, allow.clipped)) continue;
      }
      problems.push({ check: cs.textOverflow === 'ellipsis' ? 'ellipsis' : 'clipped', element: describe(el), text: text.slice(0, 60),
        scrollWidth: el.scrollWidth, clientWidth: el.clientWidth, rect: rectOf(el) });
    }
    // (b) header stays one line, help button inside the window
    var header = document.querySelector('header');
    if (header) {
      var hr = header.getBoundingClientRect();
      if (hr.height > opts.maxHeaderHeight) problems.push({ check: 'header-height', element: 'header', text: '', rect: rectOf(header), limit: opts.maxHeaderHeight });
      var help = header.querySelector('[aria-label="' + opts.helpLabel + '"]');
      if (!help) problems.push({ check: 'help-missing', element: 'header', text: opts.helpLabel });
      else if (help.getBoundingClientRect().right > innerWidth + 0.5) problems.push({ check: 'help-outside', element: describe(help), text: opts.helpLabel, rect: rectOf(help), innerWidth: innerWidth });
      // Direct header children must not run past the window or overlap their neighbour.
      var kids = Array.prototype.filter.call(header.querySelectorAll(':scope > *'), isVisible);
      for (var k = 0; k < kids.length; k++) {
        var kr = kids[k].getBoundingClientRect();
        if (kr.right > innerWidth + 0.5) problems.push({ check: 'header-child-outside', element: describe(kids[k]), text: ownText(kids[k]), rect: rectOf(kids[k]), innerWidth: innerWidth });
        if (k > 0) {
          var pr = kids[k - 1].getBoundingClientRect();
          if (kr.left < pr.right - 0.5 && kr.top < pr.bottom - 0.5 && kr.bottom > pr.top + 0.5)
            problems.push({ check: 'header-overlap', element: describe(kids[k - 1]) + ' / ' + describe(kids[k]), text: ownText(kids[k]), rect: rectOf(kids[k]) });
        }
      }
    }
    // (c) no horizontal scrolling of the document
    var root = document.documentElement;
    if (root.scrollWidth > innerWidth) problems.push({ check: 'document-scroll-x', element: 'html', text: '', scrollWidth: root.scrollWidth, innerWidth: innerWidth });
    if (document.body.scrollWidth > innerWidth) problems.push({ check: 'body-scroll-x', element: 'body', text: '', scrollWidth: document.body.scrollWidth, innerWidth: innerWidth });
    // (d) plate bar must sit below the 3D surface and not collide with the surface's overlay controls
    var groups = document.querySelectorAll('[role="radiogroup"]');
    for (var g = 0; g < groups.length; g++) {
      var grp = groups[g];
      if (!isVisible(grp)) continue;
      var plate = grp.getBoundingClientRect();
      var surfaces = document.querySelectorAll('[data-viewer-surface]');
      for (var s = 0; s < surfaces.length; s++) {
        var surface = surfaces[s];
        if (!isVisible(surface)) continue;
        var sr = surface.getBoundingClientRect();
        if (plate.top < sr.bottom - 1 && plate.bottom > sr.top && plate.left < sr.right && plate.right > sr.left)
          problems.push({ check: 'plate-bar-over-3d', element: describe(grp), text: grp.getAttribute('aria-label') || '', plate: rectOf(grp), surface: rectOf(surface) });
        var controls = document.querySelectorAll('[data-detail-viewer] button, [data-detail-viewer] [role="group"]');
        for (var c = 0; c < controls.length; c++) {
          if (grp.contains(controls[c]) || !isVisible(controls[c])) continue;
          var cr = controls[c].getBoundingClientRect();
          if (cr.left < plate.right && cr.right > plate.left && cr.top < plate.bottom && cr.bottom > plate.top)
            problems.push({ check: 'plate-bar-overlaps-control', element: describe(controls[c]), text: ownText(controls[c]) || controls[c].getAttribute('aria-label') || '', plate: rectOf(grp), control: rectOf(controls[c]) });
        }
      }
    }
    return problems;
  }

  if (kind === 'typography') {
    var glyphs = opts.glyphs;
    for (var j = 0; j < all.length; j++) {
      var node = all[j];
      var tag = node.tagName.toLowerCase();
      if (tag === 'script' || tag === 'style' || tag === 'canvas') continue;
      var own = ownText(node);
      var shown = own;
      // Form controls show their value/placeholder, which is not a text node.
      if ((tag === 'input' || tag === 'textarea') && node.type !== 'checkbox' && node.type !== 'radio') shown = (node.value || node.placeholder || '').trim();
      if (!shown || !isVisible(node)) continue;
      var style = getComputedStyle(node);
      var first = style.fontFamily.split(',')[0].trim().replace(/^["']|["']$/g, '');
      var wanted = node.closest('.font-code') ? 'IBM Plex Mono' : 'Barlow';
      if (first !== wanted && !matchesAny(node, allow.fonts))
        problems.push({ check: 'font', element: describe(node), text: shown.slice(0, 60), found: style.fontFamily, expected: wanted });
      if (!matchesAny(node, allow.glyphs)) {
        for (var q = 0; q < glyphs.length; q++) {
          if (shown.indexOf(glyphs[q]) !== -1) problems.push({ check: 'glyph', element: describe(node), text: shown.slice(0, 60), glyph: glyphs[q] });
        }
      }
      // Symbols injected through CSS content are text characters too.
      var pseudos = ['::before', '::after'];
      for (var p = 0; p < pseudos.length; p++) {
        var content = getComputedStyle(node, pseudos[p]).content;
        if (content && content !== 'none' && content !== 'normal') {
          for (var q2 = 0; q2 < glyphs.length; q2++) if (content.indexOf(glyphs[q2]) !== -1 && !matchesAny(node, allow.glyphs))
            problems.push({ check: 'glyph-pseudo', element: describe(node) + pseudos[p], text: content, glyph: glyphs[q2] });
        }
      }
    }
    return problems;
  }
  throw new Error('unknown audit kind ' + kind);
}
