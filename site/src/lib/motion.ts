/**
 * The shared motion lifecycle.
 *
 * One module owns the only requestAnimationFrame loop, so a counter, the
 * preloader and smooth scrolling never compete for frames. Smooth scrolling
 * (Lenis) and the hero readiness probe (the hero chunk) are dynamic imports:
 * they are fetched only after the reader's device has been checked, and the
 * static part of this module keeps the motion core inside its byte budget.
 *
 * GSAP and ScrollTrigger are imported statically because the reveal and
 * progress components need them on the first frame they run. A page that does
 * not import this module never downloads them. ScrollTrigger is registered
 * once, after CSSPlugin arrives.
 *
 * Every scroll-driven write happens in a ScrollTrigger callback that only
 * touches a transform. There are no scroll listeners and no layout reads in
 * an event handler.
 */

/// <reference types="gsap" />
import { gsap } from 'gsap/gsap-core';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

export const REVEAL_DISTANCE = 16;
export const REVEAL_DURATION = 500;
export const COUNTER_DURATION = 900;
export const PRELOADER_MIN = 400;
export const PRELOADER_MAX = 1200;
export const PRELOADER_HARD_LIMIT = 1500;
export const MOTION_BUDGET_BYTES = 45 * 1024;

const PRELOADER_SESSION_KEY = 'climb:preloader-seen';

/**
 * The core build leaves CSSPlugin out, but ScrollTrigger needs its prefix
 * helper and the reveal tweens need CSS property interpolation. The plugin is
 * fetched together with the rest of the motion palette, after the device has
 * been checked, so a page that does not run motion never pays for it. The
 * plugins are registered here, after CSSPlugin is present: ScrollTrigger reads
 * the prefix helper during registration.
 */
let pluginReady: Promise<void> | undefined;

function loadPlugins(): Promise<void> {
  pluginReady ??= import('gsap/CSSPlugin')
    .then(({ CSSPlugin }) => {
      gsap.registerPlugin(CSSPlugin, ScrollTrigger);
    })
    .catch(() => undefined);
  return pluginReady;
}

export type MotionMode = 'full' | 'touch' | 'reduced';

/** The reader has asked the operating system to remove non-essential motion. */
export function isReduced(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

/** A coarse pointer or any touch point means the device scrolls natively. */
export function isTouch(): boolean {
  if (typeof window === 'undefined') {
    return false;
  }
  const coarse =
    typeof window.matchMedia === 'function' && window.matchMedia('(pointer: coarse)').matches;
  return coarse || navigator.maxTouchPoints > 0;
}

export function motionMode(): MotionMode {
  if (isReduced()) {
    return 'reduced';
  }
  return isTouch() ? 'touch' : 'full';
}

/** Ease-out cubic between two values, clamped at both ends. */
export function counterValue(from: number, to: number, progress: number): number {
  const clamped = Math.min(1, Math.max(0, progress));
  const eased = 1 - Math.pow(1 - clamped, 3);
  return from + (to - from) * eased;
}

/** Map ScrollTrigger's 0..1 progress onto a transform scale. */
export function progressScale(progress: number): number {
  if (!Number.isFinite(progress)) {
    return 0;
  }
  return Math.min(1, Math.max(0, progress));
}

type Tick = (now: number, delta: number) => void;

const ticks = new Set<Tick>();
const visibilityListeners = new Set<(hidden: boolean) => void>();
let frame = 0;
let lastTime = 0;
let paused = false;

function loop(now: number): void {
  frame = 0;
  const delta = lastTime === 0 ? 0 : now - lastTime;
  lastTime = now;
  for (const tick of ticks) {
    tick(now, delta);
  }
  if (!paused && ticks.size > 0) {
    frame = requestAnimationFrame(loop);
  }
}

/** Register a frame callback; returns an unsubscribe function. */
export function onTick(tick: Tick): () => void {
  ticks.add(tick);
  if (frame === 0 && !paused) {
    frame = requestAnimationFrame(loop);
  }
  return () => {
    ticks.delete(tick);
  };
}

export function pauseTicker(): void {
  paused = true;
  if (frame !== 0) {
    cancelAnimationFrame(frame);
    frame = 0;
  }
  lastTime = 0;
}

export function resumeTicker(): void {
  if (!paused) {
    return;
  }
  paused = false;
  if (ticks.size > 0 && frame === 0) {
    frame = requestAnimationFrame(loop);
  }
}

/** Observe tab visibility. The hook is exported for components that are not
 * frame-driven, such as a CSS animation that should pause with the tab. */
export function onVisibilityChange(listener: (hidden: boolean) => void): () => void {
  visibilityListeners.add(listener);
  return () => {
    visibilityListeners.delete(listener);
  };
}

function installVisibilityPause(): void {
  const update = (): void => {
    const hidden = document.visibilityState === 'hidden';
    document.documentElement.dataset.motionPaused = hidden ? 'true' : 'false';
    for (const listener of visibilityListeners) {
      listener(hidden);
    }
    if (hidden) {
      pauseTicker();
    } else {
      resumeTicker();
    }
  };
  document.addEventListener('visibilitychange', update, { passive: true });
  update();
}

function numberFormat(decimals: number): Intl.NumberFormat {
  return new Intl.NumberFormat('en-US', {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  });
}

const formatters = new Map<number, Intl.NumberFormat>();

function decimalsOf(text: string): number {
  const match = /\.(\d+)/.exec(text);
  return match?.[1]?.length ?? 0;
}

/**
 * An intermediate counter frame. The final frame is always the text the
 * server rendered, so the animation can never land on a differently rounded
 * value. `count_of` facts animate only their numerator.
 */
export function counterFrame(value: number, finalText: string): string {
  const prefix = finalText.startsWith('≈') ? '≈' : '';
  const body = prefix === '' ? finalText : finalText.slice(1);
  const [head = body, tail] = body.split(' of ');
  const decimals = decimalsOf(head);
  let formatter = formatters.get(decimals);
  if (formatter === undefined) {
    formatter = numberFormat(decimals);
    formatters.set(decimals, formatter);
  }
  const formatted = formatter.format(value);
  return tail === undefined ? `${prefix}${formatted}` : `${prefix}${formatted} of ${tail}`;
}

function markShown(element: HTMLElement): void {
  element.dataset.revealState = 'shown';
  element.style.removeProperty('opacity');
  element.style.removeProperty('transform');
  element.style.removeProperty('translate');
}

function initReveals(root: ParentNode): void {
  const elements = [...root.querySelectorAll<HTMLElement>('[data-reveal]')];
  if (elements.length === 0) {
    return;
  }
  if (isReduced()) {
    for (const element of elements) {
      markShown(element);
    }
    return;
  }

  const pending = elements.filter((element) => element.dataset.revealState !== 'shown');
  if (pending.length === 0) {
    return;
  }

  try {
    for (const element of pending) {
      element.dataset.revealState = 'pending';
    }
    gsap.set(pending, { opacity: 0, y: REVEAL_DISTANCE });
    ScrollTrigger.batch(pending, {
      start: 'top 88%',
      once: true,
      onEnter: (batch) => {
        const entries = batch as HTMLElement[];
        gsap.to(entries, {
          opacity: 1,
          y: 0,
          duration: REVEAL_DURATION / 1000,
          ease: 'power2.out',
          stagger: 0.06,
          overwrite: true,
          onComplete: () => {
            for (const element of entries) {
              markShown(element);
            }
          },
        });
      },
    });
    ScrollTrigger.refresh();
  } catch {
    for (const element of pending) {
      markShown(element);
    }
  }
}

function initCounters(root: ParentNode): void {
  const reduced = isReduced();
  for (const element of root.querySelectorAll<HTMLElement>('[data-counter]')) {
    if (element.dataset.counterReady === 'true') {
      continue;
    }
    element.dataset.counterReady = 'true';

    const valueElement = element.querySelector<HTMLElement>('.fact-value');
    const finalText = element.dataset.counterFinal ?? valueElement?.textContent ?? '';
    if (valueElement === null || finalText === '') {
      continue;
    }

    const to = Number(element.dataset.counterTo);
    if (reduced || !Number.isFinite(to)) {
      valueElement.textContent = finalText;
      element.dataset.counterState = 'final';
      continue;
    }

    element.dataset.counterState = 'waiting';
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) {
          return;
        }
        observer.disconnect();
        element.dataset.counterState = 'counting';
        let elapsed = 0;
        const stop = onTick((_now, delta) => {
          elapsed += delta;
          const progress = Math.min(1, elapsed / COUNTER_DURATION);
          valueElement.textContent =
            progress >= 1 ? finalText : counterFrame(counterValue(0, to, progress), finalText);
          if (progress >= 1) {
            stop();
            element.dataset.counterState = 'final';
          }
        });
      },
      { threshold: 0.25 },
    );
    observer.observe(element);
  }
}

function initScrollProgress(root: ParentNode): void {
  const bar = root.querySelector<HTMLElement>('[data-scroll-progress-bar]');
  if (bar === null) {
    return;
  }
  const update = (progress: number): void => {
    bar.style.transform = `scaleX(${progressScale(progress)})`;
  };
  update(0);
  try {
    ScrollTrigger.create({
      start: 0,
      end: 'max',
      onUpdate: (self) => {
        update(self.progress);
      },
    });
    ScrollTrigger.refresh();
  } catch {
    update(0);
  }
}

function readPreloaderSession(): boolean {
  try {
    return sessionStorage.getItem(PRELOADER_SESSION_KEY) === '1';
  } catch {
    return false;
  }
}

function writePreloaderSession(): void {
  try {
    sessionStorage.setItem(PRELOADER_SESSION_KEY, '1');
  } catch {
    // Storage can be unavailable in a privacy mode; the counter still runs.
  }
}

function initPreloader(root: ParentNode): void {
  const element = root.querySelector<HTMLElement>('[data-preloader]');
  if (element === null || element.dataset.preloaderReady === 'true') {
    return;
  }
  element.dataset.preloaderReady = 'true';

  const count = element.querySelector<HTMLElement>('[data-preloader-count]');
  if (isReduced() || readPreloaderSession()) {
    element.dataset.preloaderState = 'skipped';
    element.dataset.preloaderBlockedMs = '0';
    if (count !== null) {
      count.textContent = '100';
    }
    return;
  }

  writePreloaderSession();
  element.dataset.preloaderState = 'running';

  let ready = false;
  let finished = false;
  let elapsed = 0;

  const fonts = document.fonts?.ready ?? Promise.resolve();
  const hero = import('./hero').then(({ whenHeroReady }) => whenHeroReady()).catch(() => undefined);
  Promise.allSettled([fonts, hero]).then(() => {
    ready = true;
  });

  const finish = (): void => {
    if (finished) {
      return;
    }
    finished = true;
    stop();
    if (count !== null) {
      count.textContent = '100';
    }
    element.dataset.preloaderState = 'done';
    element.dataset.preloaderBlockedMs = String(Math.round(performance.now()));
  };

  const stop = onTick((_now, delta) => {
    if (finished) {
      return;
    }
    elapsed += delta;
    const shown = Math.min(1, elapsed / PRELOADER_MAX);
    if (count !== null) {
      count.textContent = String(Math.round(shown * 100)).padStart(3, '0');
    }
    if ((ready && elapsed >= PRELOADER_MIN) || elapsed >= PRELOADER_MAX) {
      finish();
    }
  });

  // The CSS failsafe hides the overlay at PRELOADER_HARD_LIMIT even if this
  // script never runs its last frame. This timer keeps the reported state and
  // the DOM in agreement in the normal case.
  window.setTimeout(finish, PRELOADER_MAX);
}

async function initSmoothScroll(disabled: boolean): Promise<void> {
  const html = document.documentElement;
  if (disabled) {
    html.dataset.smoothScroll = 'off';
    return;
  }
  html.dataset.smoothScroll = 'on';
  try {
    const { default: Lenis } = await import('lenis');
    const lenis = new Lenis({ autoRaf: false, smoothWheel: true });
    onTick((now) => {
      lenis.raf(now);
    });
    lenis.start();
  } catch {
    html.dataset.smoothScroll = 'off';
  }
}

let installed = false;

/**
 * Initialise every motion primitive under `root`. Safe to call more than once:
 * the second call returns immediately. The page includes this through the
 * preloader component, which is the first primitive in reading order.
 */
export async function initMotion(root: ParentNode = document): Promise<void> {
  if (installed) {
    return;
  }
  installed = true;

  const mode = motionMode();
  const html = document.documentElement;
  html.dataset.motion = mode;
  html.dataset.motionReady = 'true';

  installVisibilityPause();
  initPreloader(root);
  initCounters(root);
  await loadPlugins();
  initReveals(root);
  initScrollProgress(root);
  void initSmoothScroll(mode !== 'full');
}
