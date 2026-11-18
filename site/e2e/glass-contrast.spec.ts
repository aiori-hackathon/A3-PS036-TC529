/**
 * Pixel contrast for the glass scrim.
 *
 * The stop-composite in `scripts/site-check.py contrast` is a fast pre-check:
 * it assumes the brightest orb wash sits under every text run. This suite is
 * the real rule. It renders the port and the prototype in Chromium, freezes
 * the three background orbs at the animation phase that puts each orb's centre
 * on a panel's body-text centroid, hides the text, screenshots the panel and
 * computes the WCAG ratio of the text colour against every background pixel
 * inside each text run's DOM range rectangles. It then searches the scrim from
 * 0 % to 78 % in 6 % steps and prints the worst body ratio, the worst large
 * ratio and the panel's mean absolute pixel difference from the prototype.
 *
 * Run with `npm run e2e -- glass-contrast.spec.ts`. The table is printed to the
 * test output. `GLASS_ARTIFACTS=1` additionally writes the before/after pair
 * recorded in `docs/site/skin-diffs.md`.
 *
 * What it cannot see: real GPU compositing, a physical phone, Safari or
 * Firefox, and a screen reader. It samples Chromium's software raster at a
 * device scale factor of 1.
 */
import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = fileURLToPath(new URL('.', import.meta.url));
const repo = join(here, '..', '..');
const PROTOTYPE = pathToFileURL(
  join(repo, 'docs', 'site', 'prototype', 'climb-site.html'),
).href;

const HOME = process.env.GLASS_HOME ?? '/';
const SELECTORS = [
  '#hd',
  '.led li',
  '.tr>div',
  '.lim',
  '.dg',
  '.arch',
  '.dash',
  '.tm li',
];
// The prototype has no recorded-dashboard section.
const PROTOTYPE_SELECTORS = SELECTORS.filter((selector) => selector !== '.dash');
const DURATIONS = [30, 36, 42];
const PHASES = 24;
const STEPS = Array.from({ length: 14 }, (_, index) => Number((index * 0.06).toFixed(2)));
const BODY_MIN = 4.5;
const LARGE_MIN = 3.0;
const CHOSEN = 0.36;

const STYLE = `
.gm,.gm *{color:transparent!important;text-shadow:none!important}
.gm svg text{fill:transparent!important;stroke:transparent!important}
.gm{opacity:1!important;transform:none!important}
.bgm{opacity:1!important;transform:none!important}
.bgm>*{visibility:hidden!important}
.r{opacity:1!important;transform:none!important;transition:none!important;clip-path:none!important}
*:not(.orbs i){animation-play-state:paused!important;transition:none!important}
`;

interface Run {
  x: number;
  y: number;
  w: number;
  h: number;
  colour: string;
  large: boolean;
  text: string;
}

interface Anchor {
  x: number;
  y: number;
}

interface Collected {
  runs: Run[];
  origin: Anchor;
}

interface Row {
  body: number;
  large: number;
  atBody: string;
  atLarge: string;
  mad: number[];
}

interface Measured {
  body: number;
  large: number;
  atBody: string;
  atLarge: string;
}

const frame = (page: Page): Promise<void> =>
  page.evaluate(
    () =>
      new Promise<void>((resolve) => {
        requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(resolve, 0)));
      }),
  );

async function prepare(page: Page, url: string): Promise<void> {
  await page.goto(url, { waitUntil: 'load' });
  await page.waitForTimeout(900);
  await page.evaluate(() => {
    document.documentElement.style.scrollBehavior = 'auto';
  });
  await page.addStyleTag({ content: STYLE });
  await page.evaluate(() => {
    for (const orb of document.querySelectorAll<HTMLElement>('.orbs i')) {
      orb.style.animationPlayState = 'paused';
    }
  });
  await page.evaluate(() => Promise.race([document.fonts.ready, new Promise((r) => setTimeout(r, 2000))]));
}

async function orbCentres(page: Page): Promise<Anchor[][]> {
  const centres: Anchor[][] = [];
  for (let orb = 0; orb < 3; orb += 1) {
    const list: Anchor[] = [];
    for (let step = 0; step < PHASES; step += 1) {
      const duration = DURATIONS[orb] ?? 30;
      const delay = -((step / PHASES) * duration);
      const centre = await page.evaluate(
        ({ orb, delay }) => {
          const element = document.querySelectorAll<HTMLElement>('.orbs i')[orb];
          if (!element) return { x: 0, y: 0 };
          element.style.animationDelay = `${delay}s`;
          const rect = element.getBoundingClientRect();
          return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
        },
        { orb, delay },
      );
      await frame(page);
      list.push(centre);
    }
    centres.push(list);
  }
  return centres;
}

async function setPhases(page: Page, step: number[]): Promise<void> {
  await page.evaluate(
    ({ step, durations }) => {
      document.querySelectorAll<HTMLElement>('.orbs i').forEach((element, index) => {
        const duration = durations[index] ?? 30;
        const phase = step[index] ?? 0;
        element.style.animationDelay = `${-((phase / 24) * duration)}s`;
      });
    },
    { step, durations: DURATIONS },
  );
  await frame(page);
}

function nearestStep(centres: Anchor[][], anchor: Anchor): number[] {
  return centres.map((list) =>
    list.reduce(
      (best, centre, index) => {
        const distance = (centre.x - anchor.x) ** 2 + (centre.y - anchor.y) ** 2;
        return distance < best.distance ? { distance, step: index } : best;
      },
      { distance: Number.POSITIVE_INFINITY, step: 0 },
    ).step,
  );
}

async function scrollOffsets(page: Page, selector: string, index: number): Promise<number[]> {
  return page.evaluate(
    ({ selector, index }) => {
      const element = document.querySelectorAll<HTMLElement>(selector)[index];
      if (!element) return [0];
      const max = element.scrollWidth - element.clientWidth;
      return max > 1 ? [0, max] : [0];
    },
    { selector, index },
  );
}

async function setScroll(page: Page, selector: string, index: number, left: number): Promise<void> {
  await page.evaluate(
    ({ selector, index, left }) => {
      const element = document.querySelectorAll<HTMLElement>(selector)[index];
      if (element) element.scrollLeft = left;
    },
    { selector, index, left },
  );
  await frame(page);
}

/** Runs in the panel's visible box at the current scroll offset. */
async function collect(page: Page, selector: string, index: number): Promise<Collected> {
  return page.evaluate(
    ({ selector, index }) => {
      const element = document.querySelectorAll<HTMLElement>(selector)[index];
      if (!element) return { runs: [], origin: { x: 0, y: 0 } };
      const box = element.getBoundingClientRect();
      const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
      const runs: Run[] = [];
      let node = walker.nextNode();
      while (node) {
        if (node.textContent?.trim()) {
          const parent = node.parentElement;
          if (parent) {
            let hidden = false;
            for (let item: Element | null = parent; item && item !== element.parentElement; item = item.parentElement) {
              const style = getComputedStyle(item);
              if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0) {
                hidden = true;
                break;
              }
            }
            if (!hidden) {
              const style = getComputedStyle(parent);
              const size = Number.parseFloat(style.fontSize);
              const weight = Number.parseInt(style.fontWeight, 10) || 400;
              const large = size >= 24 || (size >= 18.66 && weight >= 700);
              const tag = parent.tagName.toLowerCase();
              const colour = tag === 'text' || tag === 'tspan' ? style.fill : style.color;
              if (colour && colour !== 'none' && colour !== 'transparent') {
                const range = document.createRange();
                range.selectNodeContents(node);
                for (const rect of range.getClientRects()) {
                  if (rect.width < 2 || rect.height < 2) continue;
                  // Only text a reader can actually see: the panel may scroll.
                  if (
                    rect.left < box.left ||
                    rect.top < box.top ||
                    rect.right > box.right ||
                    rect.bottom > box.bottom
                  ) {
                    continue;
                  }
                  runs.push({
                    x: rect.x - box.x,
                    y: rect.y - box.y,
                    w: rect.width,
                    h: rect.height,
                    colour,
                    large,
                    text: (node.textContent ?? '').trim().slice(0, 28),
                  });
                }
              }
            }
          }
        }
        node = walker.nextNode();
      }
      return { runs, origin: { x: box.x, y: box.y } };
    },
    { selector, index },
  );
}

async function shot(page: Page, selector: string, index: number, className: 'gm' | 'bgm'): Promise<string> {
  await page.evaluate(
    ({ selector, index, className }) => {
      document.querySelectorAll('.gm,.bgm').forEach((element) => element.classList.remove('gm', 'bgm'));
      document.querySelectorAll<HTMLElement>(selector)[index]?.classList.add(className);
    },
    { selector, index, className },
  );
  await frame(page);
  return (await page.locator(selector).nth(index).screenshot()).toString('base64');
}

async function measure(
  page: Page,
  first: string,
  second: string | null,
  alpha: number,
  runs: Run[],
): Promise<Measured> {
  return page.evaluate(
    async ({ first, second, alpha, runs }) => {
      const decode = async (png: string): Promise<ImageData> => {
        const image = new Image();
        image.src = `data:image/png;base64,${png}`;
        await image.decode();
        const canvas = document.createElement('canvas');
        canvas.width = image.width;
        canvas.height = image.height;
        const context = canvas.getContext('2d');
        if (!context) throw new Error('no 2d context');
        context.drawImage(image, 0, 0);
        return context.getImageData(0, 0, canvas.width, canvas.height);
      };
      const start = await decode(first);
      const end = second ? await decode(second) : start;
      const channel = (value: number): number => {
        const unit = value / 255;
        return unit <= 0.04045 ? unit / 12.92 : ((unit + 0.055) / 1.055) ** 2.4;
      };
      const luminance = (red: number, green: number, blue: number): number =>
        0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue);
      const parse = (colour: string): number[] => (colour.match(/[\d.]+/g) ?? []).map(Number);
      let body = 99;
      let large = 99;
      let atBody = '';
      let atLarge = '';
      for (const run of runs) {
        const [red = 0, green = 0, blue = 0, textAlpha = 1] = parse(run.colour);
        const foreground = luminance(red, green, blue);
        const x0 = Math.max(0, Math.floor(run.x));
        const x1 = Math.min(start.width, Math.ceil(run.x + run.w));
        const y0 = Math.max(0, Math.floor(run.y));
        const y1 = Math.min(start.height, Math.ceil(run.y + run.h));
        let worst = 99;
        for (let y = y0; y < y1; y += 1) {
          for (let x = x0; x < x1; x += 1) {
            const offset = (y * start.width + x) * 4;
            // `--glass-scrim` is an alpha layer over a static backdrop, so the
            // pixel at any opacity is a linear blend of the two captures.
            const background = luminance(
              (start.data[offset] ?? 0) * (1 - alpha) + (end.data[offset] ?? 0) * alpha,
              (start.data[offset + 1] ?? 0) * (1 - alpha) + (end.data[offset + 1] ?? 0) * alpha,
              (start.data[offset + 2] ?? 0) * (1 - alpha) + (end.data[offset + 2] ?? 0) * alpha,
            );
            const text =
              textAlpha < 1 ? foreground * textAlpha + background * (1 - textAlpha) : foreground;
            const ratio = (Math.max(text, background) + 0.05) / (Math.min(text, background) + 0.05);
            if (ratio < worst) worst = ratio;
          }
        }
        if (run.large) {
          if (worst < large) {
            large = worst;
            atLarge = run.text;
          }
        } else if (worst < body) {
          body = worst;
          atBody = run.text;
        }
      }
      return { body, large, atBody, atLarge };
    },
    { first, second, alpha, runs },
  );
}

async function meanDifference(
  page: Page,
  first: string,
  second: string,
  alpha: number,
  prototype: string,
): Promise<number> {
  return page.evaluate(
    async ({ first, second, alpha, prototype }) => {
      const load = async (png: string): Promise<Uint8ClampedArray> => {
        const image = new Image();
        image.src = `data:image/png;base64,${png}`;
        await image.decode();
        const canvas = document.createElement('canvas');
        canvas.width = 64;
        canvas.height = 64;
        const context = canvas.getContext('2d');
        if (!context) throw new Error('no 2d context');
        context.drawImage(image, 0, 0, 64, 64);
        return context.getImageData(0, 0, 64, 64).data;
      };
      const start = await load(first);
      const end = await load(second);
      const reference = await load(prototype);
      let sum = 0;
      let count = 0;
      for (let offset = 0; offset < start.length; offset += 4) {
        for (let channel = 0; channel < 3; channel += 1) {
          const blended =
            (start[offset + channel] ?? 0) * (1 - alpha) + (end[offset + channel] ?? 0) * alpha;
          sum += Math.abs(blended - (reference[offset + channel] ?? 0));
          count += 1;
        }
      }
      return sum / count;
    },
    { first, second, alpha, prototype },
  );
}

async function setScrim(page: Page, value: number): Promise<void> {
  await page.evaluate((alpha) => {
    document.documentElement.style.setProperty('--glass-scrim', `rgba(7,8,10,${alpha})`);
  }, value);
  await frame(page);
}

async function measureViewport(
  page: Page,
  prototypePage: Page,
  width: number,
): Promise<{ table: Map<number, Row>; printed: string }> {
  const centres = await orbCentres(page);
  const table = new Map<number, Row>();
  for (const step of STEPS) table.set(step, { body: 99, large: 99, atBody: '', atLarge: '', mad: [] });

  for (const selector of SELECTORS) {
    const count = await page.locator(selector).count();
    const prototypeCount = PROTOTYPE_SELECTORS.includes(selector)
      ? await prototypePage.locator(selector).count()
      : 0;
    for (let index = 0; index < count; index += 1) {
      if (selector === '#hd') await page.evaluate(() => window.scrollTo(0, 0));
      else await page.locator(selector).nth(index).scrollIntoViewIfNeeded();
      await page.waitForTimeout(60);

      const offsets = await scrollOffsets(page, selector, index);
      const collected: Collected[] = [];
      for (const offset of offsets) {
        await setScroll(page, selector, index, offset);
        collected.push(await collect(page, selector, index));
      }
      const runs = collected.flatMap((item) => item.runs);
      if (!runs.length) continue;
      const bodyRuns = collected.flatMap((item) =>
        item.runs.filter((run) => !run.large).map((run) => ({ ...run, origin: item.origin })),
      );
      const area = bodyRuns.reduce((sum, run) => sum + run.w * run.h, 0) || 1;
      const anchor: Anchor = {
        x:
          bodyRuns.reduce(
            (sum, run) => sum + (run.x + run.w / 2 + run.origin.x) * run.w * run.h,
            0,
          ) / area,
        y:
          bodyRuns.reduce(
            (sum, run) => sum + (run.y + run.h / 2 + run.origin.y) * run.w * run.h,
            0,
          ) / area,
      };
      const step = nearestStep(centres, anchor);
      await setPhases(page, step);

      let prototypeBackground: string | null = null;
      if (index < prototypeCount) {
        if (selector === '#hd') await prototypePage.evaluate(() => window.scrollTo(0, 0));
        else await prototypePage.locator(selector).nth(index).scrollIntoViewIfNeeded();
        await prototypePage.waitForTimeout(60);
        await setScroll(prototypePage, selector, index, 0);
        await setPhases(prototypePage, step);
        prototypeBackground = await shot(prototypePage, selector, index, 'bgm');
      }

      // The scrim is an alpha layer over a static backdrop, so the whole
      // 0-78 % curve is the linear blend of two real captures. Capture the ends
      // once per panel, then re-capture the chosen row so the decision itself is
      // asserted on a real frame rather than on the interpolation.
      const maskedStart: string[] = [];
      const maskedEnd: string[] = [];
      const backgroundStart: string[] = [];
      const backgroundEnd: string[] = [];
      for (const offset of offsets) {
        await setScroll(page, selector, index, offset);
        await setScrim(page, 0);
        maskedStart.push(await shot(page, selector, index, 'gm'));
        backgroundStart.push(await shot(page, selector, index, 'bgm'));
        await setScrim(page, 1);
        maskedEnd.push(await shot(page, selector, index, 'gm'));
        backgroundEnd.push(await shot(page, selector, index, 'bgm'));
      }
      let panelChosenBody = 99;
      let panelChosenLarge = 99;
      for (const opacity of STEPS) {
        for (let offsetIndex = 0; offsetIndex < offsets.length; offsetIndex += 1) {
          await setScroll(page, selector, index, offsets[offsetIndex] ?? 0);
          const measured = await measure(
            page,
            maskedStart[offsetIndex] ?? '',
            maskedEnd[offsetIndex] ?? '',
            opacity,
            collected[offsetIndex]?.runs ?? [],
          );
          const row = table.get(opacity);
          if (!row) continue;
          if (measured.body < row.body) {
            row.body = measured.body;
            row.atBody = `${selector}[${index}] "${measured.atBody}"`;
          }
          if (measured.large < row.large) {
            row.large = measured.large;
            row.atLarge = `${selector}[${index}] "${measured.atLarge}"`;
          }
          if (opacity === CHOSEN) {
            panelChosenBody = Math.min(panelChosenBody, measured.body);
            panelChosenLarge = Math.min(panelChosenLarge, measured.large);
          }
        }
        if (prototypeBackground) {
          table
            .get(opacity)
            ?.mad.push(
              await meanDifference(
                page,
                backgroundStart[0] ?? '',
                backgroundEnd[0] ?? '',
                opacity,
                prototypeBackground,
              ),
            );
        }
      }

      await setScrim(page, CHOSEN);
      let realBody = 99;
      let realLarge = 99;
      let realAtBody = '';
      let realAtLarge = '';
      for (let offsetIndex = 0; offsetIndex < offsets.length; offsetIndex += 1) {
        await setScroll(page, selector, index, offsets[offsetIndex] ?? 0);
        const real = await shot(page, selector, index, 'gm');
        const measured = await measure(
          page,
          real,
          null,
          0,
          collected[offsetIndex]?.runs ?? [],
        );
        if (measured.body < realBody) {
          realBody = measured.body;
          realAtBody = measured.atBody;
        }
        if (measured.large < realLarge) {
          realLarge = measured.large;
          realAtLarge = measured.atLarge;
        }
      }
      const chosen = table.get(CHOSEN);
      if (chosen) {
        if (realBody < chosen.body) {
          chosen.body = realBody;
          chosen.atBody = `${selector}[${index}] "${realAtBody}"`;
        }
        if (realLarge < chosen.large) {
          chosen.large = realLarge;
          chosen.atLarge = `${selector}[${index}] "${realAtLarge}"`;
        }
        // The two real endpoints and the blend must agree; a larger gap means
        // the backdrop moved between captures and the panel must be re-measured.
        expect(
          Math.abs(panelChosenBody - realBody),
          `interpolation vs real frame at ${selector}[${index}]`,
        ).toBeLessThan(0.25);
        expect(
          Math.abs(panelChosenLarge - realLarge),
          `large interpolation vs real frame at ${selector}[${index}]`,
        ).toBeLessThan(0.25);
      }
      await page.evaluate(
        ({ selector, index }) => {
          document.querySelectorAll<HTMLElement>(selector)[index]?.classList.remove('gm', 'bgm');
        },
        { selector, index },
      );
    }
  }

  const lines = [
    `${width} px viewport`,
    'scrim  worst-body  worst-large  MAD vs prototype  worst body / worst large',
  ];
  for (const opacity of STEPS) {
    const row = table.get(opacity);
    if (!row) continue;
    const mad = row.mad.length ? row.mad.reduce((sum, value) => sum + value, 0) / row.mad.length : 0;
    lines.push(
      `${String(Math.round(opacity * 100)).padStart(4)}%  ${row.body.toFixed(2).padStart(10)}  ${row.large
        .toFixed(2)
        .padStart(11)}  ${mad.toFixed(2).padStart(16)}  ${row.atBody} / ${row.atLarge}`,
    );
  }
  const printed = lines.join('\n');
  console.log(`\n${printed}\n`);
  return { table, printed };
}

async function assertChosen(table: Map<number, Row>, viewport: string): Promise<void> {
  const row = table.get(CHOSEN);
  expect(row, `no row for the chosen ${CHOSEN * 100} % scrim`).toBeDefined();
  expect(
    row?.body ?? 0,
    `worst body text at ${CHOSEN * 100} % on ${viewport}: ${row?.atBody}`,
  ).toBeGreaterThanOrEqual(BODY_MIN);
  expect(
    row?.large ?? 0,
    `worst large text at ${CHOSEN * 100} % on ${viewport}: ${row?.atLarge}`,
  ).toBeGreaterThanOrEqual(LARGE_MIN);
  const unscrimmed = table.get(0);
  expect(
    unscrimmed?.body ?? 99,
    'the prototype glass must fail at least one body run, or the scrim is not justified',
  ).toBeLessThan(BODY_MIN);
  for (const [opacity, step] of table) {
    if (opacity < CHOSEN) continue;
    expect(step.body, `body at ${opacity * 100} %`).toBeGreaterThanOrEqual(BODY_MIN);
    expect(step.large, `large at ${opacity * 100} %`).toBeGreaterThanOrEqual(LARGE_MIN);
  }
}

async function declaredScrim(page: Page): Promise<number> {
  const value = await page.evaluate(() =>
    getComputedStyle(document.documentElement).getPropertyValue('--glass-scrim').trim(),
  );
  // Chromium serialises the token as #07080ac7; other versions use rgba().
  if (/^#[0-9a-f]{8}$/i.test(value)) return Number.parseInt(value.slice(7, 9), 16) / 255;
  const match = value.match(/rgba\(\s*7\s*,\s*8\s*,\s*10\s*,\s*([0-9.]+)\s*\)/);
  expect(match, `--glass-scrim is not the ink scrim: ${value}`).not.toBeNull();
  return Number(match?.[1] ?? '0');
}

for (const viewport of [
  { width: 1440, height: 900 },
  { width: 390, height: 844 },
] as const) {
  test(`glass scrim is the measured minimum at ${viewport.width}x${viewport.height}`, async ({
    page,
    context,
  }) => {
    test.setTimeout(35 * 60 * 1000);
    await page.setViewportSize(viewport);
    await prepare(page, HOME);
    expect(await declaredScrim(page)).toBeCloseTo(CHOSEN, 2);

    const prototypePage = await context.newPage();
    await prototypePage.setViewportSize(viewport);
    await prepare(prototypePage, PROTOTYPE);

    const { table } = await measureViewport(page, prototypePage, viewport.width);
    await assertChosen(table, `${viewport.width}x${viewport.height}`);

    if (process.env.GLASS_ARTIFACTS === '1' && viewport.width === 1440) {
      const selector = '.led li';
      await page.locator(selector).first().scrollIntoViewIfNeeded();
      await setPhases(page, nearestStep(await orbCentres(page), { x: viewport.width / 2, y: 300 }));
      await setScrim(page, 0.78);
      await page.screenshot({ path: join(repo, 'docs', 'site', 'glass-before.png') });
      await setScrim(page, CHOSEN);
      await page.screenshot({ path: join(repo, 'docs', 'site', 'glass-after.png') });
    }
  });
}
