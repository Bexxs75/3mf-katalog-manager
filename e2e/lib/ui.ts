// Real-input helpers. Everything goes through the W3C Actions API, i.e. the
// browser's own pointer/keyboard pipeline, so focus, hover, mousedown/mouseup
// ordering and re-renders between mousedown and mouseup behave as for a user.
// element.click() from WebDriver is also real input, but does not let a test
// press and release separately; execute('el.click()') is never used.
import type { ChainablePromiseElement } from 'webdriverio';

export type El = WebdriverIO.Element | ChainablePromiseElement;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/**
 * Scrolls the element into view with a DOM call. Scrolling itself is not what
 * these tests check, and WebDriver's wheel-based scrolling fails with "move
 * target out of bounds" for elements inside nested scrollers.
 */
export async function reveal(el: El) {
  const target = await el;
  await browser.execute((e: HTMLElement) => e.scrollIntoView({ block: 'center', inline: 'center' }), target as unknown as HTMLElement);
}

/** Mouse click: move, press, short hold, release. */
export async function click(el: El, button: 0 | 2 = 0) {
  const target = await el;
  await reveal(target);
  await browser.action('pointer', { parameters: { pointerType: 'mouse' } })
    .move({ origin: target }).down({ button }).pause(60).up({ button }).perform();
}

export const rightClick = (el: El) => click(el, 2);

/** Only the move, so hover styles can be inspected. */
export async function hover(el: El) {
  const target = await el;
  await browser.action('pointer', { parameters: { pointerType: 'mouse' } }).move({ origin: target }).perform();
}

/**
 * Press on `from`, move in small steps over to `to`, release. Many steps with
 * short pauses because the app starts a drag only after a movement threshold
 * and highlights drop targets on mouseenter.
 */
export async function dragAndDrop(from: El, to: El, opts: { steps?: number; grabOffset?: { x: number; y: number } } = {}) {
  const src = (await from) as WebdriverIO.Element, dst = (await to) as WebdriverIO.Element;
  await reveal(src);
  const a = await src.getLocation(), as = await src.getSize();
  const b = await dst.getLocation(), bs = await dst.getSize();
  const off = opts.grabOffset ?? { x: Math.round(as.width / 2), y: Math.round(as.height / 2) };
  const start = { x: Math.round(a.x + off.x), y: Math.round(a.y + off.y) };
  const end = { x: Math.round(b.x + bs.width / 2), y: Math.round(b.y + bs.height / 2) };
  const steps = opts.steps ?? 12;
  const pointer = browser.action('pointer', { parameters: { pointerType: 'mouse' } })
    .move({ x: start.x, y: start.y }).down().pause(80);
  for (let i = 1; i <= steps; i++) {
    pointer.move({ x: Math.round(start.x + ((end.x - start.x) * i) / steps), y: Math.round(start.y + ((end.y - start.y) * i) / steps), duration: 30 });
  }
  await pointer.pause(120).up().perform();
}

/** Types text with the keyboard action source (not setValue). */
export async function typeKeys(text: string) {
  await browser.keys(text.split(''));
}

export async function pressKey(key: string) {
  await browser.keys([key]);
}

/**
 * First element with the given tag whose text (trimmed, whitespace collapsed) equals `text`.
 * One XPath request instead of a loop over getText(): the catalog re-renders
 * often (thumbnails arrive, counters change) and a loop trips over stale elements.
 */
export async function byText(tag: string, text: string) {
  const el = await $(`//${tag}[normalize-space()=${xpathLiteral(text)}]`);
  return (await el.isExisting()) ? el : null;
}

export function xpathLiteral(text: string): string {
  if (!text.includes("'")) return `'${text}'`;
  return `concat(${text.split("'").map((part) => `'${part}'`).join(`, "'", `)})`;
}

export async function waitForText(tag: string, text: string, timeout = 10000) {
  const el = await $(`//${tag}[normalize-space()=${xpathLiteral(text)}]`);
  await el.waitForExist({ timeout, timeoutMsg: `no <${tag}> with text "${text}"` });
  return el;
}

export { sleep };
