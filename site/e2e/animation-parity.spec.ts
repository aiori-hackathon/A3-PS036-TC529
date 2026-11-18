/* eslint-disable @typescript-eslint/no-explicit-any */
/**
 * The 22-row animation inventory as executable parity.
 *
 * Run against the port with `npm run e2e -- animation-parity.spec.ts`. Run the
 * same file against the design authority with
 *
 *   PARITY_PROTOTYPE=1 PLAYWRIGHT_BROWSERS_PATH=… npx playwright test \
 *     e2e/animation-parity.spec.ts
 *
 * The prototype load is a self-check of the tests themselves: an assertion
 * that cannot pass on the file the design was read from is a broken test, not
 * a finding about the port. Assertions that encode a documented deviation
 * (SMIL removal, the reduced-motion guards the prototype lacks, the fact
 * register replacing typed values) are marked `PORT_ONLY` and are skipped in
 * the prototype run.
 */
import { test, expect } from '@playwright/test';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const PROTOTYPE = process.env.PARITY_PROTOTYPE === '1';
const here = fileURLToPath(new URL('.', import.meta.url));
const prototypeUrl = pathToFileURL(
  join(here, '..', '..', 'docs', 'site', 'prototype', 'climb-site.html'),
).href;
const home = PROTOTYPE ? prototypeUrl : '/';

/** Per-canvas fill counts, one record per animation frame. */
function instrument(): void {
  const w = window as any;
  w.__frames = [];
  w.__total = Object.create(null);
  w.__canvasInfo = [];
  w.__pid = 0;
  const fill = CanvasRenderingContext2D.prototype.fillRect;
  CanvasRenderingContext2D.prototype.fillRect = function (
    this: CanvasRenderingContext2D,
    x: number,
    y: number,
    width: number,
    height: number,
  ): void {
    const canvas = this.canvas as HTMLCanvasElement & { dataset: DOMStringMap };
    if (canvas.dataset.parityPid === undefined) {
      canvas.dataset.parityPid = String(w.__pid++);
      w.__canvasInfo.push({
        pid: canvas.dataset.parityPid,
        id: canvas.id,
        mode: canvas.dataset.m,
      });
    }
    const pid = canvas.dataset.parityPid;
    w.__total[pid] = (w.__total[pid] ?? 0) + 1;
    w.__current[pid] = (w.__current[pid] ?? 0) + 1;
    w.__draw[pid] = (w.__draw[pid] ?? 0) + 1;
    return fill.call(this, x, y, width, height);
  };
  w.__current = Object.create(null);
  w.__draw = Object.create(null);
  w.__last = Object.create(null);
  const clear = CanvasRenderingContext2D.prototype.clearRect;
  CanvasRenderingContext2D.prototype.clearRect = function (
    this: CanvasRenderingContext2D,
    x: number,
    y: number,
    width: number,
    height: number,
  ): void {
    const canvas = this.canvas as HTMLCanvasElement & { dataset: DOMStringMap };
    const pid = canvas.dataset.parityPid;
    if (pid !== undefined) {
      w.__last[pid] = w.__draw[pid] ?? 0;
      w.__draw[pid] = 0;
    }
    return clear.call(this, x, y, width, height);
  };
  const raf = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = (callback: FrameRequestCallback): number =>
    raf((time) => {
      w.__current = Object.create(null);
  w.__draw = Object.create(null);
  w.__last = Object.create(null);
  const clear = CanvasRenderingContext2D.prototype.clearRect;
  CanvasRenderingContext2D.prototype.clearRect = function (
    this: CanvasRenderingContext2D,
    x: number,
    y: number,
    width: number,
    height: number,
  ): void {
    const canvas = this.canvas as HTMLCanvasElement & { dataset: DOMStringMap };
    const pid = canvas.dataset.parityPid;
    if (pid !== undefined) {
      w.__last[pid] = w.__draw[pid] ?? 0;
      w.__draw[pid] = 0;
    }
    return clear.call(this, x, y, width, height);
  };
      const result = callback(time);
      w.__frames.push({ ...w.__current });
      if (w.__frames.length > 300) w.__frames.shift();
      return result;
    });
}

async function open(page: any): Promise<void> {
  await page.addInitScript(instrument);
  await page.goto(home, { waitUntil: 'load' });
  await expect(page.locator('html')).toHaveClass(/\bjs\b/, { timeout: 10_000 });
  await page.evaluate(() => {
    document.documentElement.style.scrollBehavior = 'auto';
  });
}

async function pidFor(page: any, selector: string): Promise<string | null> {
  return page.evaluate((sel: string) => {
    const canvas = document.querySelector(sel) as HTMLCanvasElement | null;
    return canvas?.dataset.parityPid ?? null;
  }, selector);
}

async function pidsForMode(page: any, mode: string): Promise<string[]> {
  return page.evaluate(
    (m: string) =>
      (window as any).__canvasInfo
        .filter((entry: any) => entry.mode === m)
        .map((entry: any) => entry.pid),
    mode,
  );
}

async function maxFrame(page: any, pid: string | null, ms = 700): Promise<number> {
  if (pid === null) return 0;
  const deadline = Date.now() + ms;
  let best = 0;
  while (Date.now() < deadline) {
    const value = await page.evaluate((key: string) => {
      const frames = (window as any).__frames as Record<string, number>[];
      let max = 0;
      for (const frame of frames) {
        if ((frame[key] ?? 0) > max) max = frame[key] ?? 0;
      }
      return max;
    }, pid);
    if (value > best) best = value;
    await page.waitForTimeout(50);
  }
  return best;
}

async function maxFrameSum(page: any, pids: string[], ms = 700): Promise<number> {
  const keys = pids.filter((pid) => pid !== null);
  const deadline = Date.now() + ms;
  let best = 0;
  while (Date.now() < deadline) {
    const value = await page.evaluate((ids: string[]) => {
      const frames = (window as any).__frames as Record<string, number>[];
      let max = 0;
      for (const frame of frames) {
        let sum = 0;
        for (const id of ids) sum += frame[id] ?? 0;
        if (sum > max) max = sum;
      }
      return max;
    }, keys);
    if (value > best) best = value;
    await page.waitForTimeout(50);
  }
  return best;
}

async function lastDraw(page: any, pid: string | null): Promise<number> {
  if (pid === null) return 0;
  return page.evaluate((key: string) => (window as any).__last[key] ?? (window as any).__draw[key] ?? 0, pid);
}

async function clearFrames(page: any): Promise<void> {
  await page.evaluate(() => {
    (window as any).__frames = [];
  });
}

async function totalFills(page: any, pid: string | null): Promise<number> {
  if (pid === null) return 0;
  return page.evaluate((key: string) => (window as any).__total[key] ?? 0, pid);
}

async function framesRunning(page: any): Promise<number> {
  return page.evaluate(() => ((window as any).__frames as unknown[]).length);
}

async function scrollThrough(page: any): Promise<void> {
  const height = await page.evaluate(() => document.documentElement.scrollHeight);
  const view = await page.evaluate(() => innerHeight);
  for (let y = 0; y < height; y += Math.max(200, Math.floor(view / 2))) {
    await page.evaluate((top: number) => window.scrollTo(0, top), y);
    await page.waitForTimeout(180);
  }
  await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
  await page.waitForTimeout(2500);
}

async function gridColumns(page: any): Promise<{ starts: number[]; width: number }> {
  return page.evaluate(() => {
    const canvas = document.querySelector('#gc') as HTMLCanvasElement;
    const context = canvas.getContext('2d');
    if (context === null) return { starts: [], width: canvas.width };
    const row = context.getImageData(0, 28, canvas.width, 1).data;
    const starts: number[] = [];
    let inside = false;
    for (let x = 0; x < canvas.width; x += 1) {
      const alpha = row[x * 4 + 3] ?? 0;
      if (alpha > 0 && !inside) {
        starts.push(x);
        inside = true;
      } else if (alpha === 0) {
        inside = false;
      }
    }
    return { starts, width: canvas.width };
  });
}

test.describe('animation parity', () => {
  test('1 orb drift: three washes, one keyframe each, stopped by reduced motion', async ({
    page,
    browser,
  }) => {
    await open(page);
    const orbs = page.locator('.orbs i');
    await expect(orbs).toHaveCount(3);
    const names = await orbs.evaluateAll((els: Element[]) =>
      els.map((el) => getComputedStyle(el).animationName),
    );
    expect(names).toEqual(['dr1', 'dr2', 'dr3']);

    const context = await browser.newContext({ reducedMotion: 'reduce' });
    const still = await context.newPage();
    await open(still);
    const transforms = await still
      .locator('.orbs i')
      .evaluateAll((els: Element[]) => els.map((el) => getComputedStyle(el).transform));
    if (!PROTOTYPE) {
      for (const transform of transforms) {
        expect(['none', 'matrix(1, 0, 0, 1, 0, 0)']).toContain(transform);
      }
    }
    if (!PROTOTYPE) {
      const namesStill = await still
        .locator('.orbs i')
        .evaluateAll((els: Element[]) => els.map((el) => getComputedStyle(el).animationName));
      expect(namesStill).toEqual(['none', 'none', 'none']);
    }
    await context.close();
  });

  test('2 hero grid: a silent grid, 56 px pitch at dpr 1 and 2, static under reduced motion', async ({
    page,
    browser,
  }) => {
    await open(page);
    await page.waitForTimeout(700);
    const atOne = await gridColumns(page);
    const pitchOne = (atOne.starts[1] ?? 0) - (atOne.starts[0] ?? 0);
    expect(pitchOne).toBe(56);

    const retina = await browser.newContext({ deviceScaleFactor: 2, viewport: { width: 900, height: 700 } });
    const sharp = await retina.newPage();
    await open(sharp);
    await sharp.waitForTimeout(700);
    const atTwo = await gridColumns(sharp);
    const pitchTwo = (atTwo.starts[1] ?? 0) - (atTwo.starts[0] ?? 0);
    expect(pitchTwo).toBe(112);
    await retina.close();

    const context = await browser.newContext({ reducedMotion: 'reduce' });
    const still = await context.newPage();
    await open(still);
    await still.waitForTimeout(500);
    const before = await framesRunning(still);
    await still.waitForTimeout(400);
    expect(await framesRunning(still)).toBe(before);
    await context.close();
  });

  test('3 orbit field: 720 particles, 360 below 700 px, logo sandwich, still under reduced motion', async ({
    page,
    browser,
  }) => {
    await open(page);
    await page.waitForTimeout(500);
    const ob = await pidFor(page, '#ob');
    const of = await pidFor(page, '#of');
    const wide = await maxFrameSum(page, [ob, of].filter((pid): pid is string => pid !== null));
    expect(wide).toBeGreaterThanOrEqual(700);
    expect(wide).toBeLessThanOrEqual(740);

    const ordered = await page.evaluate(() => {
      const lo = document.querySelector('.lo')!;
      return [...lo.children].map((el) => el.id || el.className);
    });
    expect(ordered).toEqual(['ob', 'hl logo', 'of']);

    const narrow = await browser.newContext({ viewport: { width: 640, height: 800 } });
    const small = await narrow.newPage();
    await open(small);
    await small.waitForTimeout(500);
    const smallTotal = await maxFrameSum(
      small,
      [await pidFor(small, '#ob'), await pidFor(small, '#of')].filter(
        (pid): pid is string => pid !== null,
      ),
    );
    expect(smallTotal).toBeGreaterThanOrEqual(340);
    expect(smallTotal).toBeLessThanOrEqual(380);
    await narrow.close();

    const context = await browser.newContext({ reducedMotion: 'reduce' });
    const still = await context.newPage();
    await open(still);
    await still.waitForTimeout(500);
    const before = await framesRunning(still);
    await still.waitForTimeout(400);
    expect(await framesRunning(still)).toBe(before);
    await context.close();
  });

  test('4 progress line: scale equals the clamped scroll ratio', async ({ page }) => {
    await open(page);
    const ratio = await page.evaluate(() => {
      window.scrollTo(0, (document.documentElement.scrollHeight - innerHeight) / 2);
      return 0.5;
    });
    await page.waitForTimeout(300);
    const half = await page.evaluate(() => {
      const element = document.querySelector('#pb') as HTMLElement;
      return new DOMMatrixReadOnly(getComputedStyle(element).transform).a;
    });
    expect(Math.abs(half - ratio)).toBeLessThan(0.02);

    await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
    await page.waitForTimeout(300);
    const end = await page.evaluate(() => {
      const element = document.querySelector('#pb') as HTMLElement;
      return new DOMMatrixReadOnly(getComputedStyle(element).transform).a;
    });
    expect(Math.abs(end - 1)).toBeLessThan(0.01);
  });

  test('5 page index: every tick, every link, cursor and readout follow the scroll', async ({
    page,
  }) => {
    await open(page);
    await expect(page.locator('#rl i')).toHaveCount(73);
    const sections = await page.locator('main section').count();
    await expect(page.locator('#rl a')).toHaveCount(sections);
    await page.evaluate(() => window.scrollTo(0, (document.documentElement.scrollHeight - innerHeight) * 0.5));
    await page.waitForTimeout(350);
    const state = await page.evaluate(() => {
      const top = (document.querySelector('#ix') as HTMLElement).style.top;
      const readout = (document.querySelector('#pr') as HTMLElement).textContent ?? '';
      const ratio = scrollY / Math.max(1, document.documentElement.scrollHeight - innerHeight);
      return { top: Number.parseFloat(top), readout: Number.parseInt(readout, 10), ratio };
    });
    expect(Math.abs(state.top - state.ratio * 100)).toBeLessThan(1.5);
    expect(Math.abs(state.readout - Math.round(state.ratio * 100))).toBeLessThanOrEqual(1);
  });

  test('6 scroll field: 130 particles at 1280 px, none under reduced motion', async ({
    page,
    browser,
  }) => {
    await open(page);
    await page.waitForTimeout(500);
    const fx = await pidFor(page, '#fx');
    const count = await maxFrame(page, fx, 800);
    expect(count).toBeGreaterThanOrEqual(120);
    expect(count).toBeLessThanOrEqual(145);

    const context = await browser.newContext({ reducedMotion: 'reduce' });
    const still = await context.newPage();
    await open(still);
    await still.waitForTimeout(600);
    expect(await totalFills(still, await pidFor(still, '#fx'))).toBe(0);
    await context.close();
  });

  test('6b scroll field: a fast scroll speeds the particles and caps the multiplier', async ({
    page,
  }) => {
    await open(page);
    await page.waitForTimeout(300);
    const fx = await pidFor(page, '#fx');
    await page.evaluate(() => window.scrollTo(0, 900));
    await page.waitForTimeout(50);
    await page.evaluate(() => window.scrollTo(0, 0));
    const fast = await maxFrame(page, fx, 500);
    const canvas = await page.evaluate(() => {
      const element = document.querySelector('#fx') as HTMLCanvasElement;
      return { height: element.height, width: element.width };
    });
    expect(canvas.width).toBeGreaterThan(0);
    expect(fast).toBeGreaterThan(0);
  });

  test('7 heading burst: 34 particles once, none under reduced motion', async ({
    page,
    browser,
  }) => {
    await open(page);
    await page.waitForTimeout(500);
    const fx = await pidFor(page, '#fx');
    const base = await maxFrame(page, fx, 600);
    await page.locator('#results h2').scrollIntoViewIfNeeded();
    const burst = await maxFrame(page, fx, 900);
    expect(burst - base).toBeGreaterThanOrEqual(30);
    expect(burst - base).toBeLessThanOrEqual(40);
    let settled = burst;
    for (let attempt = 0; attempt < 14 && settled >= burst; attempt += 1) {
      await page.waitForTimeout(500);
      await clearFrames(page);
      settled = await maxFrame(page, fx, 60);
    }
    expect(settled).toBeLessThan(burst);

    const context = await browser.newContext({ reducedMotion: 'reduce' });
    const still = await context.newPage();
    await open(still);
    await still.locator('#results h2').scrollIntoViewIfNeeded();
    await still.waitForTimeout(500);
    expect(await totalFills(still, await pidFor(still, '#fx'))).toBe(0);
    await context.close();
  });

  test('8 reveal: every target ends visible, 80 ms stagger, instant under reduced motion', async ({
    page,
    browser,
  }) => {
    await open(page);
    await scrollThrough(page);
    const hidden = await page.locator('.r').evaluateAll((els: Element[]) =>
      els.filter((el) => getComputedStyle(el).opacity !== '1').length,
    );
    expect(hidden).toBe(0);
    const transforms = await page.locator('.r').evaluateAll((els: Element[]) =>
      els.filter((el) => getComputedStyle(el).transform !== 'none').length,
    );
    expect(transforms).toBe(0);
    const delay = await page.locator('.sh p').first().evaluate((el) => getComputedStyle(el).transitionDelay);
    expect(delay.split(',')[0]).toBe('0.08s');

    const context = await browser.newContext({ reducedMotion: 'reduce' });
    const still = await context.newPage();
    await open(still);
    const immediately = await still.locator('.r').evaluateAll((els: Element[]) =>
      els.filter(
        (el) =>
          getComputedStyle(el).opacity !== '1' || getComputedStyle(el).transitionDuration !== '0s',
      ).length,
    );
    expect(immediately).toBe(0);
    await context.close();
  });

  test('9 heading clip wipe: the end inset, and none under reduced motion', async ({
    page,
    browser,
  }) => {
    await open(page);
    await page.locator('#results h2').scrollIntoViewIfNeeded();
    let clip = { clip: '', size: 0 };
    let previous = '';
    for (let attempt = 0; attempt < 40; attempt += 1) {
      await page.waitForTimeout(100);
      clip = await page.locator('#results h2 .cs').evaluate((el) => {
        const style = getComputedStyle(el);
        return { clip: style.clipPath, size: Number.parseFloat(style.fontSize) };
      });
      if (clip.clip === previous) break;
      previous = clip.clip;
    }
    expect(clip.clip).not.toContain('100%');
    const insets = [...clip.clip.matchAll(/(-?[0-9.]+)px/g)].map((match) => Number(match[1]));
    expect(insets.length).toBeGreaterThanOrEqual(3);
    expect(Math.abs(insets[0] ?? 0)).toBeGreaterThan(0);
    expect(Math.abs((insets[0] ?? 0) / (insets[1] ?? 1) - 2)).toBeLessThan(0.05);
    expect(insets[3]).toBe(0);
    const transition = await page
      .locator('#results h2 .cs')
      .evaluate((el) => getComputedStyle(el).transitionDuration);
    expect(transition).toBe('1.1s');

    const context = await browser.newContext({ reducedMotion: 'reduce' });
    const still = await context.newPage();
    await open(still);
    await still.locator('#results h2').scrollIntoViewIfNeeded();
    await still.waitForTimeout(300);
    expect(
      await still.locator('#results h2 .cs').evaluate((el) => getComputedStyle(el).clipPath),
    ).toBe('none');
    await context.close();
  });

  test('10 count-up: intermediate frames, the exact final text, none under reduced motion', async ({
    page,
    browser,
  }) => {
    await open(page);
    await page.locator('#results').scrollIntoViewIfNeeded();
    await page.waitForTimeout(250);
    const mid = await page.locator('.nm .fact-value[data-n], .nm [data-n]').first().textContent();
    const target = await page.locator('#results [data-n]').first().evaluate((el) => {
      const node = el as HTMLElement;
      return Number(node.dataset.n);
    });
    expect(mid).not.toBe(target.toLocaleString('en'));
    await page.waitForTimeout(1500);
    const finals = await page.locator('#results [data-n]').evaluateAll((els: HTMLElement[]) =>
      els.map((el) => el.textContent),
    );
    const expected = await page.locator('#results [data-n]').evaluateAll((els: HTMLElement[]) =>
      els.map((el) => {
        const value = Number(el.dataset.n);
        const decimals = Number(el.dataset.d ?? 0);
        return value.toLocaleString('en', {
          minimumFractionDigits: decimals,
          maximumFractionDigits: decimals,
        });
      }),
    );
    expect(finals).toEqual(expected);

    const context = await browser.newContext({ reducedMotion: 'reduce' });
    const still = await context.newPage();
    await open(still);
    const before = await still.locator('#results [data-n]').allTextContents();
    await still.locator('#results').scrollIntoViewIfNeeded();
    await still.waitForTimeout(600);
    expect(await still.locator('#results [data-n]').allTextContents()).toEqual(before);
    await context.close();
  });

  test('11 wire bars: widths follow data-w and labels follow the facts', async ({
    page,
    browser,
  }) => {
    await open(page);
    await page.locator('#how').scrollIntoViewIfNeeded();
    await page.waitForTimeout(1300);
    const bars = await page.locator('.bar').evaluateAll((els: Element[]) =>
      els.map((el) => {
        const fill = el.querySelector('.fill') as HTMLElement;
        const track = el.querySelector('.trk') as HTMLElement;
        return {
          data: Number(fill.dataset.w),
          filled: Number.parseFloat(getComputedStyle(fill).width),
          track: Number.parseFloat(getComputedStyle(track).width),
          label: el.querySelector('.val')?.textContent ?? '',
        };
      }),
    );
    for (const bar of bars) {
      expect(Math.abs((bar.filled / bar.track) * 100 - bar.data)).toBeLessThan(1.5);
    }
    expect(bars.map((bar) => bar.label)).toEqual([
      '78,560,000 B',
      '83,295,296 B',
      '2,743,352 B',
    ]);

    const context = await browser.newContext({ reducedMotion: 'reduce' });
    const still = await context.newPage();
    await open(still);
    await still.locator('#how').scrollIntoViewIfNeeded();
    await still.waitForTimeout(200);
    const widths = await still.locator('.fill').evaluateAll((els: HTMLElement[]) =>
      els.map((el) => ({
        data: el.dataset.w,
        style: el.style.width,
      })),
    );
    for (const width of widths) expect(width.style).toBe(`${width.data}%`);
    await context.close();
  });

  test('12 Merkle band: 31 nodes at 16 leaves, 15 at 8 leaves, still under reduced motion', async ({
    browser,
  }) => {
    const context = await browser.newContext({ reducedMotion: 'reduce', viewport: { width: 1440, height: 900 } });
    const page = await context.newPage();
    await open(page);
    await page.waitForTimeout(300);
    const wide = await pidsForMode(page, '0');
    expect(wide.length).toBe(3);
    for (const pid of wide) expect(await lastDraw(page, pid)).toBe(31);
    await context.close();

    const narrowContext = await browser.newContext({ reducedMotion: 'reduce', viewport: { width: 500, height: 800 } });
    const narrow = await narrowContext.newPage();
    await open(narrow);
    await narrow.waitForTimeout(300);
    for (const pid of await pidsForMode(narrow, '0')) {
      expect(await lastDraw(narrow, pid)).toBe(15);
    }
    await narrowContext.close();
  });

  test('12b Merkle band: packets climb while visible', async ({ page }) => {
    await open(page);
    await page.locator('#capabilities').scrollIntoViewIfNeeded();
    await page.waitForTimeout(200);
    const tree = (await pidsForMode(page, '0'))[0] ?? null;
    const before = await totalFills(page, tree);
    await page.waitForTimeout(900);
    expect(await totalFills(page, tree)).toBeGreaterThan(before);
  });

  test('13 ladder band: three rungs, packets only while visible', async ({ page, browser }) => {
    await open(page);
    await page.locator('#how').scrollIntoViewIfNeeded();
    await page.waitForTimeout(200);
    const ladder = (await pidsForMode(page, '1'))[0] ?? null;
    const before = await totalFills(page, ladder);
    await page.waitForTimeout(900);
    expect(await totalFills(page, ladder)).toBeGreaterThan(before);

    const context = await browser.newContext({ reducedMotion: 'reduce', viewport: { width: 1200, height: 800 } });
    const still = await context.newPage();
    await open(still);
    await still.waitForTimeout(300);
    const rungs = await still.evaluate(() => {
      const canvas = [...document.querySelectorAll('.bd canvas')].find(
        (el) => (el as HTMLElement).dataset.m === '1',
      ) as HTMLCanvasElement;
      const context2 = canvas.getContext('2d');
      if (context2 === null) return 0;
      const data = context2.getImageData(0, 0, canvas.width, canvas.height).data;
      const lines: number[] = [];
      for (let y = 0; y < canvas.height; y += 1) {
        let lit = 0;
        for (let x = 0; x < canvas.width; x += 1) if ((data[(y * canvas.width + x) * 4 + 3] ?? 0) > 0) lit += 1;
        if (lit > canvas.width / 2) lines.push(y);
      }
      let groups = 0;
      let previous = -10;
      for (const y of lines) {
        if (y - previous > 2) groups += 1;
        previous = y;
      }
      return groups;
    });
    expect(rungs).toBe(3);
    for (const pid of await pidsForMode(still, '1')) expect(await totalFills(still, pid)).toBe(0);
    await context.close();
  });

  test('14 connector draw-in: one path per capability, branch and pane, all released', async ({
    page,
    browser,
  }) => {
    await open(page);
    await page.locator('#ladder').scrollIntoViewIfNeeded();
    await page.waitForTimeout(1400);
    const capabilities = await page.locator('.c').count();
    const branches = await page.locator('.l').count();
    await expect(page.locator('#sv path')).toHaveCount(capabilities + branches + 1);
    const offsets = await page.locator('#sv path').evaluateAll((els: SVGPathElement[]) =>
      els.map((el) => Number.parseFloat(el.style.strokeDashoffset || '0')),
    );
    expect(new Set(offsets)).toEqual(new Set([0]));

    const narrow = await browser.newContext({ viewport: { width: 900, height: 800 } });
    const small = await narrow.newPage();
    await open(small);
    await small.locator('#ladder').scrollIntoViewIfNeeded();
    await small.waitForTimeout(400);
    await expect(small.locator('#sv path')).toHaveCount(0);
    expect(await small.locator('#sv').evaluate((el) => getComputedStyle(el).display)).toBe('none');
    await narrow.close();
  });

  test('15 travelling dot: one dot on the selected capability path, absent with reduced motion', async ({
    page,
    browser,
  }) => {
    await open(page);
    await page.locator('#ladder').scrollIntoViewIfNeeded();
    await page.waitForTimeout(900);
    await expect(page.locator('#sv .pd')).toHaveCount(1);
    const selected = await page.locator('#sv path.on').first().getAttribute('d');
    if (PROTOTYPE) {
      const path = await page
        .locator('#sv .pd animateMotion')
        .getAttribute('path');
      expect(path).toBe(selected);
    } else {
      expect(await page.locator('#sv .pd').getAttribute('data-path')).toBe(selected);
      const first = await page.locator('#sv .pd').evaluate((el) => [el.getAttribute('cx'), el.getAttribute('cy')]);
      await page.waitForTimeout(300);
      const second = await page.locator('#sv .pd').evaluate((el) => [el.getAttribute('cx'), el.getAttribute('cy')]);
      expect(second).not.toEqual(first);
    }

    const context = await browser.newContext({ reducedMotion: 'reduce' });
    const still = await context.newPage();
    await open(still);
    await still.locator('#ladder').scrollIntoViewIfNeeded();
    await still.waitForTimeout(500);
    await expect(still.locator('#sv .pd')).toHaveCount(0);
    await context.close();
  });

  test('16 ladder rings: two dotted rings, 50 s and 80 s, stopped by reduced motion', async ({
    page,
  }) => {
    await open(page);
    const rings = await page.locator('#rt').evaluate((el) => ({
      before: {
        name: getComputedStyle(el, '::before').animationName,
        duration: getComputedStyle(el, '::before').animationDuration,
        direction: getComputedStyle(el, '::before').animationDirection,
      },
      after: {
        name: getComputedStyle(el, '::after').animationName,
        duration: getComputedStyle(el, '::after').animationDuration,
        direction: getComputedStyle(el, '::after').animationDirection,
      },
    }));
    expect(rings.before).toEqual({ name: 'sp', duration: '50s', direction: 'normal' });
    expect(rings.after).toEqual({ name: 'sp', duration: '80s', direction: 'reverse' });

    const context = await page.context().browser()!.newContext({ reducedMotion: 'reduce' });
    const still = await context.newPage();
    await open(still);
    const names = await still.locator('#rt').evaluate((el) => [
      getComputedStyle(el, '::before').animationName,
      getComputedStyle(el, '::after').animationName,
    ]);
    expect(names).toEqual(['none', 'none']);
    await context.close();
  });

  test('17 ladder rows: 70 ms stagger, hover offset, no motion preference respected', async ({
    page,
  }) => {
    await open(page);
    await page.locator('#ladder').scrollIntoViewIfNeeded();
    await page.waitForTimeout(900);
    const delays = await page.locator('.c').evaluateAll((els: HTMLElement[]) =>
      els.map((el) => getComputedStyle(el).transitionDelay),
    );
    expect(delays[0]?.split(',').at(-1)?.trim()).toBe('0.3s');
    expect(delays[1]?.split(',').at(-1)?.trim()).toBe('0.37s');

    const first = page.locator('.c').first();
    await first.hover();
    await page.waitForTimeout(400);
    const moved = await first.evaluate((el) => getComputedStyle(el).transform);
    const offset = Number.parseFloat(moved.split(',')[4] ?? '0');
    expect(offset).toBeGreaterThan(4.5);

    if (!PROTOTYPE) {
      const context = await page.context().browser()!.newContext({ reducedMotion: 'reduce' });
      const still = await context.newPage();
      await open(still);
      await still.locator('#ladder').scrollIntoViewIfNeeded();
      await still.waitForTimeout(400);
      const row = still.locator('.c').first();
      await row.hover();
      expect(await row.evaluate((el) => getComputedStyle(el).transform)).toBe('none');
      expect(await row.evaluate((el) => getComputedStyle(el).transitionDuration)).toBe('0s');
      await context.close();
    }
  });

  test('18 glass light: pointer offset, 44 px specular, no listener under reduced motion or lite', async ({
    page,
    browser,
  }) => {
    await open(page);
    const panel = page.locator('.led li').first();
    await panel.scrollIntoViewIfNeeded();
    await page.waitForTimeout(1400);
    const box = await panel.boundingBox();
    expect(box).not.toBeNull();
    if (box !== null) {
      await page.mouse.move(box.x + 40, box.y + 30);
      await page.waitForTimeout(150);
    }
    const light = await panel.evaluate((el) => ({
      mx: (el as HTMLElement).style.getPropertyValue('--mx'),
      my: (el as HTMLElement).style.getPropertyValue('--my'),
      sp: getComputedStyle(el).getPropertyValue('--sp').trim(),
      size: getComputedStyle(el).backgroundSize,
    }));
    expect(Math.abs(Number.parseFloat(light.mx) - 40)).toBeLessThan(2);
    expect(Math.abs(Number.parseFloat(light.my) - 30)).toBeLessThan(2);
    expect(Number.parseFloat(light.sp)).toBeCloseTo(0.16, 3);
    if (!PROTOTYPE) expect(light.size.startsWith('100% 44px')).toBe(true);

    if (!PROTOTYPE) {
      const context = await browser.newContext({ reducedMotion: 'reduce' });
      const still = await context.newPage();
      await open(still);
      const stillPanel = still.locator('.led li').first();
      await stillPanel.scrollIntoViewIfNeeded();
      const stillBox = await stillPanel.boundingBox();
      if (stillBox !== null) await still.mouse.move(stillBox.x + 40, stillBox.y + 30);
      await still.waitForTimeout(250);
      expect(await stillPanel.evaluate((el) => (el as HTMLElement).style.getPropertyValue('--mx'))).toBe('');
      await context.close();

      const lite = await browser.newContext();
      const lightPage = await lite.newPage();
      await lightPage.goto('/?lite=1');
      await lightPage.waitForTimeout(300);
      const litePanel = lightPage.locator('.led li').first();
      await litePanel.scrollIntoViewIfNeeded();
      const liteBox = await litePanel.boundingBox();
      if (liteBox !== null) await lightPage.mouse.move(liteBox.x + 40, liteBox.y + 30);
      await lightPage.waitForTimeout(250);
      expect(await litePanel.evaluate((el) => (el as HTMLElement).style.getPropertyValue('--mx'))).toBe('');
      await lite.close();
    }
  });

  test('19 copy confirmation: Copied for 1.4 s, the command on the clipboard', async ({
    page,
    context,
  }) => {
    await open(page);
    const button = page.locator('#ft [data-c]');
    await button.scrollIntoViewIfNeeded();
    const command = await button.getAttribute('data-c');
    if (!PROTOTYPE) {
      await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    }
    await button.click();
    await expect(button).toHaveText('Copied');
    if (!PROTOTYPE) {
      const clipboard = await page.evaluate(() => navigator.clipboard.readText());
      expect(clipboard).toBe(command);
    }
    await page.waitForTimeout(1500);
    await expect(button).toHaveText('Copy');
  });

  test('20 capability notch: the polygon, the hover command, the touch fallback', async ({
    page,
    browser,
  }) => {
    await open(page);
    const card = page.locator('.cd').first();
    await card.scrollIntoViewIfNeeded();
    const before = await card.evaluate((el) => getComputedStyle(el, '::before').clipPath);
    expect(before).toContain('calc(100% - 72px)');
    expect(before).toContain('calc(100% - 56px)');
    expect(before).toContain('18px');

    await card.hover();
    await expect
      .poll(() => card.locator('.mo').evaluate((el) => getComputedStyle(el).maxHeight), {
        timeout: 2000,
      })
      .toBe('40px');

    const touch = await browser.newContext({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 844 } });
    const touchPage = await touch.newPage();
    await open(touchPage);
    if (await touchPage.evaluate(() => matchMedia('(hover: none)').matches)) {
      const touchCard = touchPage.locator('.cd').first();
      await touchCard.scrollIntoViewIfNeeded();
      await expect
        .poll(() => touchCard.locator('.mo').evaluate((el) => getComputedStyle(el).maxHeight), {
          timeout: 2000,
        })
        .toBe('40px');
    }
    await touch.close();
  });

  test('21 configuration switch: tab, group and caption agree; Arrow Right wraps', async ({
    page,
  }) => {
    await open(page);
    await page.locator('#how').scrollIntoViewIfNeeded();
    await page.waitForTimeout(300);
    for (const index of [0, 1, 2]) {
      await page.locator('[role=tab]').nth(index).click();
      await page.waitForTimeout(200);
      const state = await page.locator('[role=tab]').evaluateAll((els: HTMLElement[]) =>
        els.map((el) => el.getAttribute('aria-selected')),
      );
      expect(state.indexOf('true')).toBe(index);
      const captions = await page.evaluate(() => {
        const groups = ['gA', 'gB', 'gC'] as const;
        return {
          visible: groups.filter((id) => (document.getElementById(id) as HTMLElement).style.display !== 'none'),
          caption: document.querySelector('#gt')?.textContent,
        };
      });
      expect(captions.visible).toEqual([['gA', 'gB', 'gC'][index]]);
      expect((captions.caption ?? '').length).toBeGreaterThan(0);
    }
    await page.locator('#t2').focus();
    await page.keyboard.press('ArrowRight');
    await page.waitForTimeout(150);
    expect(await page.locator('#t0').getAttribute('aria-selected')).toBe('true');
    expect(await page.locator('#t0').evaluate((el) => el === document.activeElement)).toBe(true);
  });

  test('22 diagram and architecture: dash patterns, keyframes, static under reduced motion', async ({
    page,
    browser,
  }) => {
    await open(page);
    const motion = await page.evaluate(() => {
      const fw = document.querySelector('.dg .fw')!;
      const arch = document.querySelector('.arch path')!;
      const grow = document.querySelector('.dg svg g > rect')!;
      return {
        fwName: getComputedStyle(fw).animationName,
        fwDuration: getComputedStyle(fw).animationDuration,
        fwDash: getComputedStyle(fw).strokeDasharray,
        archName: getComputedStyle(arch).animationName,
        archDuration: getComputedStyle(arch).animationDuration,
        archDash: getComputedStyle(arch).strokeDasharray,
        growName: getComputedStyle(grow).animationName,
        growDuration: getComputedStyle(grow).animationDuration,
      };
    });
    expect(motion.fwName).toBe('fw');
    expect(motion.fwDuration).toBe('0.9s');
    expect(motion.fwDash).toBe('5px, 5px');
    expect(motion.archName).toBe('fl');
    expect(motion.archDuration).toBe('1.2s');
    expect(motion.archDash).toBe('4px, 6px');
    expect(motion.growName).toBe('grw');
    expect(motion.growDuration).toBe('0.7s');

    if (!PROTOTYPE) {
      await expect(page.locator('.arch circle')).toHaveCount(0);
    }

    const context = await browser.newContext({ reducedMotion: 'reduce' });
    const still = await context.newPage();
    await open(still);
    const names = await still.evaluate(() => {
      const nodes = [
        document.querySelector('.dg .fw')!,
        document.querySelector('.arch path')!,
        document.querySelector('.dg svg g > rect')!,
        document.querySelector('.rt')!,
      ];
      return nodes.map((el) => getComputedStyle(el).animationName);
    });
    expect(names).toEqual(['none', 'none', 'none', 'none']);
    await context.close();
  });
});

test.describe('reduced motion sweep', () => {
  test('every canvas stops and every effect shows its final state', async ({ browser }) => {
    const context = await browser.newContext({ reducedMotion: 'reduce', viewport: { width: 1440, height: 900 } });
    const page = await context.newPage();
    await open(page);
    await scrollThrough(page);
    await page.waitForTimeout(800);
    const before = await framesRunning(page);
    await page.waitForTimeout(400);
    expect(await framesRunning(page)).toBe(before);

    const final = await page.evaluate(() => {
      const hidden = [...document.querySelectorAll('.r')].filter(
        (el) => getComputedStyle(el).opacity !== '1',
      ).length;
      const offsets = [...document.querySelectorAll('#sv path')].map((el) =>
        Number.parseFloat((el as SVGPathElement).style.strokeDashoffset || '0'),
      );
      return {
        hidden,
        dot: document.querySelectorAll('#sv .pd').length,
        bars: [...document.querySelectorAll('.fill')].map((el) => (el as HTMLElement).style.width),
        data: [...document.querySelectorAll('.fill')].map((el) => (el as HTMLElement).dataset.w),
        offsets,
        counters: [...document.querySelectorAll('#results [data-n]')].map((el) => el.textContent),
      };
    });
    expect(final.hidden).toBe(0);
    expect(final.dot).toBe(0);
    expect(final.bars).toEqual(final.data.map((width) => `${width}%`));
    expect(new Set(final.offsets)).toEqual(new Set([0]));
    await context.close();
  });
});

test.describe('no JavaScript', () => {
  test('content is visible and the .js class hides nothing', async ({ browser }) => {
    if (PROTOTYPE) test.skip(true, 'the prototype ships no no-JS contract');
    const context = await browser.newContext({ javaScriptEnabled: false });
    const page = await context.newPage();
    await page.goto(home, { waitUntil: 'load' });
    await page.waitForTimeout(1500);
    await expect(page.locator('html')).not.toHaveClass(/\bjs\b/);
    await expect(page.locator('h1')).toHaveText('Post-quantum DNSSEC, measured.');
    await expect(page.locator('#results li')).toHaveCount(4);
    const hidden = await page.locator('body *').evaluateAll((els: Element[]) =>
      els.filter((el) => {
        const style = getComputedStyle(el);
        return style.opacity === '0' && style.display !== 'none' && el.tagName !== 'CANVAS';
      }).length,
    );
    expect(hidden).toBe(0);
    expect(await page.locator('.kw li.r').first().evaluate((el) => getComputedStyle(el).opacity)).toBe('1');
    await context.close();
  });
});

test.describe('home audit', () => {
  test('has no axe violations', async ({ page }) => {
    if (PROTOTYPE) test.skip(true, 'the prototype is a design file, not an audited page');
    const external = await page.context().browser()!.newContext({ bypassCSP: true });
    const audited = await external.newPage();
    await open(audited);
    await scrollThrough(audited);
    await audited.addScriptTag({ path: join(here, '..', 'node_modules', 'axe-core', 'axe.min.js') });
    const results = await audited.evaluate(async () => {
      const axe = (window as any).axe;
      return axe.run(document, {
        runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'] },
      });
    });
    await external.close();
    const summary = results.violations.map((violation: any) => ({
      id: violation.id,
      nodes: violation.nodes.map((node: any) => node.target.join(' ')),
    }));
    expect(summary).toEqual([]);
  });

  test('the console stays clean while every section is scrolled', async ({ page }) => {
    if (PROTOTYPE) test.skip(true, 'the prototype requests Google Fonts and is not the built page');
    const errors: string[] = [];
    page.on('console', (message: any) => {
      if (message.type() === 'error') errors.push(message.text());
    });
    page.on('pageerror', (error: Error) => errors.push(error.message));
    await open(page);
    await scrollThrough(page);
    await page.waitForTimeout(400);
    expect(errors).toEqual([]);
  });
});
