/*!
 * figuren.js — shared mascot SVGs for 3mfkatalog.de
 *
 * Three figures (printer, filament spool, nozzle), each in two poses:
 *   - "point"  pointing finger, for the "We need you" hint on the home page
 *   - "thumbs" thumbs up with green sparks and a wider smile, for the thank-you
 *              after submitting the test page (druckertest.html)
 *
 * Body, head, eyes and the other (static) arm are the same in both poses; only
 * the mouth and the moving arm group differ. That's why each figure lives here
 * once instead of once per page.
 *
 * API (window.MMKFiguren):
 *   FIGURES                list of figure keys: ['printer', 'spool', 'nozzle']
 *   POSES                  list of poses: ['point', 'thumbs']
 *   random()               a random entry from FIGURES
 *   svg(figure, pose)      SVG markup as a string: an <svg class="fig" viewBox="0 0 150 170"
 *                          aria-hidden="true">…</svg>. Throws an Error for an unknown
 *                          figure/pose.
 *
 * Usage: <script src="figuren.js"></script> (from a subpage e.g. "../figuren.js")
 * BEFORE the script that calls svg()/random().
 *
 * This module does NOT animate; each including page's own <style> does: the
 * returned markup contains class="arm" for the moving arm group and class="blink"
 * for the eyes. Example (thumbs-up bobbing):
 *   .arm{transform-box:fill-box;transform-origin:center;animation:wipp 2.6s ease-in-out infinite}
 *   @keyframes wipp{0%,70%,100%{transform:translateY(0)}78%{transform:translateY(-6px)}...}
 *   .blink{animation:blink 4s infinite;transform-box:fill-box;transform-origin:center}
 *   @keyframes blink{0%,94%,100%{transform:scaleY(1)}96%{transform:scaleY(.1)}}
 *   @media (prefers-reduced-motion:reduce){.arm,.blink{animation:none}}
 * Each page only includes ONE pose, so a single .arm rule per page is enough
 * (see druckertest.html for "thumbs").
 */
(function (global) {
  'use strict';

  var VIEWBOX = '0 0 150 170';

  // Mouth per pose and figure (only the "d" attribute of the mouth path).
  var MOUTH = {
    point: {
      printer: 'M58 118q12 10 24 0',
      spool: 'M64 106q11 9 22 0',
      nozzle: 'M64 104q11 9 22 0',
    },
    thumbs: {
      printer: 'M58 118q12 14 24 0',
      spool: 'M64 106q11 13 22 0',
      nozzle: 'M64 104q11 13 22 0',
    },
  };

  // Moving arm group per pose and figure (content of <g class="arm">…</g>).
  var ARM = {
    point: {
      printer:
        '<path d="M26 100 Q30 126 50 134" stroke="#4a433d" stroke-width="10" fill="none" stroke-linecap="round"/>' +
        '<circle cx="54" cy="132" r="20" fill="#d9d2ca" stroke="#1a0d08" stroke-opacity=".25" stroke-width="2"/>' +
        '<path d="M38 125q8-5 16 0M38 134q8-5 16 0M38 143q8-5 16 0" stroke="#1a0d08" stroke-opacity=".3" stroke-width="2" fill="none"/>' +
        '<circle cx="62" cy="126" r="12" fill="#d9d2ca" stroke="#1a0d08" stroke-opacity=".3" stroke-width="2"/>' +
        '<ellipse cx="64" cy="123.5" rx="5" ry="4" fill="#fff" opacity=".85"/>' +
        '<path d="M80 112l7-5M82 126h9M80 140l7 5" stroke="#ff7a5c" stroke-width="3" stroke-linecap="round"/>',
      spool:
        '<path d="M24 96 Q26 124 50 134" stroke="#3a3530" stroke-width="10" fill="none" stroke-linecap="round"/>' +
        '<circle cx="54" cy="132" r="20" fill="#d9d2ca" stroke="#1a0d08" stroke-opacity=".25" stroke-width="2"/>' +
        '<path d="M38 125q8-5 16 0M38 134q8-5 16 0M38 143q8-5 16 0" stroke="#1a0d08" stroke-opacity=".3" stroke-width="2" fill="none"/>' +
        '<circle cx="62" cy="126" r="12" fill="#d9d2ca" stroke="#1a0d08" stroke-opacity=".3" stroke-width="2"/>' +
        '<ellipse cx="64" cy="123.5" rx="5" ry="4" fill="#fff" opacity=".85"/>' +
        '<path d="M80 112l7-5M82 126h9M80 140l7 5" stroke="#ff7a5c" stroke-width="3" stroke-linecap="round"/>',
      nozzle:
        '<path d="M36 92 Q34 120 50 134" stroke="#9d958c" stroke-width="10" fill="none" stroke-linecap="round"/>' +
        '<circle cx="54" cy="132" r="20" fill="#efeae4" stroke="#1a0d08" stroke-opacity=".25" stroke-width="2"/>' +
        '<path d="M38 125q8-5 16 0M38 134q8-5 16 0M38 143q8-5 16 0" stroke="#1a0d08" stroke-opacity=".3" stroke-width="2" fill="none"/>' +
        '<circle cx="62" cy="126" r="12" fill="#efeae4" stroke="#1a0d08" stroke-opacity=".3" stroke-width="2"/>' +
        '<ellipse cx="64" cy="123.5" rx="5" ry="4" fill="#fff" opacity=".85"/>' +
        '<path d="M80 112l7-5M82 126h9M80 140l7 5" stroke="#ff7a5c" stroke-width="3" stroke-linecap="round"/>',
    },
    thumbs: {
      printer:
        '<path d="M26 100 Q20 110 6 98" stroke="#4a433d" stroke-width="10" fill="none" stroke-linecap="round"/>' +
        '<rect x="-12" y="86" width="30" height="26" rx="9" fill="#d9d2ca" stroke="#1a0d08" stroke-opacity=".25" stroke-width="2"/>' +
        '<path d="M-8 94h22M-8 101h22" stroke="#1a0d08" stroke-opacity=".25" stroke-width="2"/>' +
        '<rect x="-9" y="62" width="12" height="30" rx="6" fill="#d9d2ca" stroke="#1a0d08" stroke-opacity=".25" stroke-width="2"/>' +
        '<path d="M-18 60l-6-6M-3 50v-8M12 60l6-6" stroke="#4cc38a" stroke-width="3" stroke-linecap="round"/>',
      spool:
        '<path d="M24 96 Q18 108 6 98" stroke="#3a3530" stroke-width="10" fill="none" stroke-linecap="round"/>' +
        '<rect x="-12" y="86" width="30" height="26" rx="9" fill="#d9d2ca" stroke="#1a0d08" stroke-opacity=".25" stroke-width="2"/>' +
        '<path d="M-8 94h22M-8 101h22" stroke="#1a0d08" stroke-opacity=".25" stroke-width="2"/>' +
        '<rect x="-9" y="62" width="12" height="30" rx="6" fill="#d9d2ca" stroke="#1a0d08" stroke-opacity=".25" stroke-width="2"/>' +
        '<path d="M-18 60l-6-6M-3 50v-8M12 60l6-6" stroke="#4cc38a" stroke-width="3" stroke-linecap="round"/>',
      nozzle:
        '<path d="M36 92 Q26 104 6 98" stroke="#9d958c" stroke-width="10" fill="none" stroke-linecap="round"/>' +
        '<rect x="-12" y="86" width="30" height="26" rx="9" fill="#efeae4" stroke="#1a0d08" stroke-opacity=".25" stroke-width="2"/>' +
        '<path d="M-8 94h22M-8 101h22" stroke="#1a0d08" stroke-opacity=".25" stroke-width="2"/>' +
        '<rect x="-9" y="62" width="12" height="30" rx="6" fill="#efeae4" stroke="#1a0d08" stroke-opacity=".25" stroke-width="2"/>' +
        '<path d="M-18 60l-6-6M-3 50v-8M12 60l6-6" stroke="#4cc38a" stroke-width="3" stroke-linecap="round"/>',
    },
  };

  // Body per figure (everything except mouth and arm group). svg() inserts mouth/arm.
  var BODY = {
    printer: function (mouth, arm) {
      return (
        '<rect x="22" y="40" width="96" height="104" rx="10" fill="#2a2622" stroke="#4a433d" stroke-width="3"/>' +
        '<rect x="30" y="48" width="80" height="16" rx="4" fill="#ff7a5c"/>' +
        '<rect x="62" y="58" width="16" height="14" rx="3" fill="#d9d2ca"/>' +
        '<path d="M66 72h8l-4 7z" fill="#ff7a5c"/>' +
        '<ellipse class="blink" cx="52" cy="95" rx="9" ry="10" fill="#efeae4"/><circle cx="54" cy="97" r="4.5" fill="#1a0d08"/>' +
        '<ellipse class="blink" cx="88" cy="95" rx="9" ry="10" fill="#efeae4"/><circle cx="86" cy="97" r="4.5" fill="#1a0d08"/>' +
        '<path d="M44 80q8-6 16-1M80 79q8-5 16 1" stroke="#1a0d08" stroke-width="3" fill="none" stroke-linecap="round"/>' +
        '<path d="' + mouth + '" stroke="#1a0d08" stroke-width="3.5" fill="none" stroke-linecap="round"/>' +
        '<rect x="30" y="128" width="80" height="8" rx="3" fill="#4cc38a" opacity=".9"/>' +
        '<rect x="28" y="144" width="12" height="18" rx="4" fill="#3a3530"/><rect x="100" y="144" width="12" height="18" rx="4" fill="#3a3530"/>' +
        '<g class="arm">' + arm + '</g>' +
        '<path d="M118 100 Q134 112 128 126" stroke="#4a433d" stroke-width="9" fill="none" stroke-linecap="round"/>' +
        '<circle cx="128" cy="128" r="8" fill="#d9d2ca"/>'
      );
    },
    spool: function (mouth, arm) {
      return (
        '<circle cx="75" cy="90" r="56" fill="#3a3530" stroke="#4a433d" stroke-width="3"/>' +
        '<circle cx="75" cy="90" r="46" fill="#ff7a5c"/>' +
        '<g stroke="#e0502f" stroke-width="2" fill="none" opacity=".7">' +
        '<circle cx="75" cy="90" r="40"/><circle cx="75" cy="90" r="34"/><circle cx="75" cy="90" r="28"/>' +
        '</g>' +
        '<circle cx="75" cy="90" r="20" fill="#2a2622" stroke="#4a433d" stroke-width="3"/>' +
        '<ellipse class="blink" cx="60" cy="80" rx="9" ry="10" fill="#efeae4"/><circle cx="62" cy="82" r="4.5" fill="#1a0d08"/>' +
        '<ellipse class="blink" cx="90" cy="80" rx="9" ry="10" fill="#efeae4"/><circle cx="88" cy="82" r="4.5" fill="#1a0d08"/>' +
        '<path d="M52 66q8-6 16-1M82 65q8-5 16 1" stroke="#1a0d08" stroke-width="3" fill="none" stroke-linecap="round"/>' +
        '<path d="' + mouth + '" stroke="#1a0d08" stroke-width="3.5" fill="none" stroke-linecap="round"/>' +
        '<path d="M118 120 Q140 140 124 160" stroke="#ff7a5c" stroke-width="3" fill="none"/>' +
        '<g class="arm">' + arm + '</g>' +
        '<rect x="52" y="144" width="12" height="20" rx="4" fill="#3a3530"/><rect x="86" y="144" width="12" height="20" rx="4" fill="#3a3530"/>'
      );
    },
    nozzle: function (mouth, arm) {
      return (
        '<rect x="40" y="22" width="70" height="34" rx="6" fill="#4a433d"/>' +
        '<g fill="#3a3530"><rect x="44" y="26" width="6" height="26"/><rect x="56" y="26" width="6" height="26"/>' +
        '<rect x="68" y="26" width="6" height="26"/><rect x="80" y="26" width="6" height="26"/><rect x="92" y="26" width="6" height="26"/></g>' +
        '<rect x="34" y="58" width="82" height="62" rx="10" fill="#d9d2ca"/>' +
        '<ellipse class="blink" cx="60" cy="84" rx="9" ry="10" fill="#fff"/><circle cx="62" cy="86" r="4.5" fill="#1a0d08"/>' +
        '<ellipse class="blink" cx="90" cy="84" rx="9" ry="10" fill="#fff"/><circle cx="88" cy="86" r="4.5" fill="#1a0d08"/>' +
        '<path d="M52 70q8-6 16-1M82 69q8-5 16 1" stroke="#1a0d08" stroke-width="3" fill="none" stroke-linecap="round"/>' +
        '<path d="' + mouth + '" stroke="#1a0d08" stroke-width="3.5" fill="none" stroke-linecap="round"/>' +
        '<path d="M60 120h30l-8 20h-14z" fill="#f2c14e"/>' +
        '<path d="M71 140h8l-4 8z" fill="#f2c14e"/>' +
        '<path d="M75 150 q-3 8 2 14" stroke="#ff7a5c" stroke-width="4" fill="none" stroke-linecap="round"/>' +
        '<g class="arm">' + arm + '</g>'
      );
    },
  };

  var FIGURES = ['printer', 'spool', 'nozzle'];
  var POSES = ['point', 'thumbs'];

  function random() {
    return FIGURES[Math.floor(Math.random() * FIGURES.length)];
  }

  function svg(figure, pose) {
    if (BODY[figure] === undefined) throw new Error('figuren.js: unbekannte Figur "' + figure + '"');
    if (MOUTH[pose] === undefined) throw new Error('figuren.js: unbekannte Pose "' + pose + '"');
    var inner = BODY[figure](MOUTH[pose][figure], ARM[pose][figure]);
    return '<svg class="fig" viewBox="' + VIEWBOX + '" aria-hidden="true">' + inner + '</svg>';
  }

  global.MMKFiguren = { FIGURES: FIGURES, POSES: POSES, random: random, svg: svg };
})(window);
