"""Helpers shared by the recipes: in-page finders, markers and crop rectangles."""
from __future__ import annotations

import asyncio
import json

C = json.dumps

# Finder and marker functions that live in the page. They look elements up by their
# visible text or aria-label so that the recipes stay readable.
HELPERS = r"""
window.__txt = (t, sel) => [...document.querySelectorAll(sel || 'button, a, [role=tab], [role=menuitem], label, h2, h3, h4, span, div, li')]
  .filter(e => (e.innerText || '').trim() === t && e.getBoundingClientRect().width > 0).pop();
window.__starts = (t, sel) => [...document.querySelectorAll(sel || 'button')]
  .filter(e => (e.innerText || '').trim().startsWith(t) && e.getBoundingClientRect().width > 0).pop();
window.__lbl = (t) => [...document.querySelectorAll('[aria-label], [title]')]
  .filter(e => (e.getAttribute('aria-label') || e.getAttribute('title') || '').trim() === t && e.getBoundingClientRect().width > 0).pop();
window.__box = (el) => { let e = el; while (e && e.parentElement && e.getBoundingClientRect().height < 260) e = e.parentElement; return e; };
window.__dialog = () => [...document.querySelectorAll('[role=dialog]')].filter(e => e.getBoundingClientRect().width > 0).pop();
window.__setInput = (el, value) => {
  const proto = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, value);
  el.dispatchEvent(new Event('input', { bubbles: true }));
};
window.__mark = (items) => {
  document.querySelectorAll('.__mk').forEach(e => e.remove());
  const missing = [];
  items.forEach(([find, n], idx) => {
    let el; try { el = find(); } catch (e) { el = null; }
    if (!el) { missing.push(idx); return; }
    const r = el.getBoundingClientRect(), pad = 5;
    const box = document.createElement('div'); box.className = '__mk';
    Object.assign(box.style, { position: 'fixed', left: (r.x - pad) + 'px', top: (r.y - pad) + 'px', width: (r.width + 2 * pad) + 'px',
      height: (r.height + 2 * pad) + 'px', border: '4px solid #e0261b', borderRadius: '10px', zIndex: 2147483646, pointerEvents: 'none' });
    document.body.appendChild(box);
    if (n) {
      const b = document.createElement('div'); b.className = '__mk'; b.textContent = n;
      // Outside the frame, to the left (or above when there is no room), so it never covers a label.
      const left = r.x - pad - 44 >= 4 ? r.x - pad - 44 : r.x - pad;
      const top = r.x - pad - 44 >= 4 ? r.y + r.height / 2 - 18 : r.y - pad - 42;
      Object.assign(b.style, { position: 'fixed', left: left + 'px', top: top + 'px', width: '36px', height: '36px', borderRadius: '50%',
        background: '#e0261b', color: '#fff', font: '700 20px/36px system-ui, sans-serif', textAlign: 'center', zIndex: 2147483647,
        pointerEvents: 'none', boxShadow: '0 2px 6px rgba(0,0,0,.35)' });
      document.body.appendChild(b);
    }
  });
  return missing;
};
window.__has = (t, sel) => [...document.querySelectorAll(sel || 'button, a, label, span, div, li')]
  .filter(e => (e.innerText || '').includes(t) && e.getBoundingClientRect().width > 0)
  .sort((a, b) => { const ra = a.getBoundingClientRect(), rb = b.getBoundingClientRect(); return ra.width * ra.height - rb.width * rb.height; })[0];
window.__tab = (t) => { const g = [...document.querySelectorAll('button, [role=tab]')].find(e => (e.innerText || '').trim() === window.__GENERAL && e.getBoundingClientRect().width > 0);
  return g && [...g.parentElement.querySelectorAll('button, [role=tab]')].find(e => (e.innerText || '').trim() === t); };
window.__panel = (el, minW) => { let e = el; while (e && e.parentElement) { const r = e.getBoundingClientRect(); if (r.width >= (minW || 520) && r.height >= 200) return e; e = e.parentElement; } return e; };
window.__fixed = (el) => { let e = el; while (e && e !== document.body && getComputedStyle(e).position !== 'fixed') e = e.parentElement; return e; };
window.__card = (t) => [...document.querySelectorAll('*')].filter(e => e.children.length === 0 && (e.innerText || '').trim() === t)
  .find(e => { const r = e.getBoundingClientRect(); return r.x > 300 && r.width > 0; });
window.__rail = (t) => window.__lbl(t) && (window.__lbl(t).closest('button') || window.__lbl(t));
window.__unclamp = () => {
  // The settings popover is a fixed 540 px high and scrolls inside. For the picture it grows to its content
  // (it is anchored at the bottom, so it grows upwards), as all pictures of it always showed the whole content.
  const t = window.__tab(window.__GENERAL); let e = t;
  while (e && e !== document.body) {
    const s = getComputedStyle(e);
    if (/(auto|scroll)/.test(s.overflowY)) { e.style.height = 'auto'; e.style.maxHeight = 'none'; e.style.overflow = 'visible'; return true; }
    e = e.parentElement;
  }
  return false;
};
window.__unmark = () => document.querySelectorAll('.__mk').forEach(e => e.remove());
window.__leaf = (t, extra) => [...document.querySelectorAll('*')].find(e => e.children.length === 0 && (e.innerText || '').trim() === t && (!extra || extra(e)));
window.__btn = (t, f) => [...document.querySelectorAll('button')].find(e => (e.textContent || '').trim() === t && (!f || f(e.getBoundingClientRect())));
window.__expand = async () => {
  // open every folder of the tree: the toggle in the folder header flips between "collapse all" and "expand all"
  for (let i = 0; i < 3; i++) {
    if ([...document.querySelectorAll('[role=button]')].some(e => e.getBoundingClientRect().x < 320 && parseInt(e.style.paddingLeft || '0') > 12)) return true;
    const b = [...document.querySelectorAll('button[aria-label]')].find(e => e.getBoundingClientRect().x < 320 && e.getBoundingClientRect().y < 260 && /^(Alle Ordner|Collapse all|Expand all|All folders)/i.test(e.getAttribute('aria-label')));
    if (!b) return false;
    b.click(); await new Promise(r => setTimeout(r, 400));
  }
  return false;
};
"""


def union_expr(elements: list[str], pad: float | tuple = 0.0) -> str:
    """JS expression for a rectangle around the given elements (JS expressions).
    `pad` in CSS px: one value for all sides or (left, top, right, bottom)."""
    l, t, r, b = (pad, pad, pad, pad) if isinstance(pad, (int, float)) else pad
    return ("(() => { const es = [" + ",".join(elements) + "].filter(Boolean); if (!es.length) return null;"
            " const rs = es.map(e => e.getBoundingClientRect()); const x = Math.min(...rs.map(r => r.x)) - %f, y = Math.min(...rs.map(r => r.y)) - %f,"
            " r = Math.max(...rs.map(r => r.right)) + %f, b = Math.max(...rs.map(r => r.bottom)) + %f;"
            " return { getBoundingClientRect: () => ({ x, y, width: r - x, height: b - y, right: r, bottom: b }) }; })()") % (l, t, r, b)


async def js(p, expr: str, wait: float = 0.8):
    r = await p.js(expr)
    await asyncio.sleep(wait)
    return r
