/**
 * The ripple surface lifecycle.
 *
 * Geometry is pure and takes no DOM, so it can be checked without a layout
 * engine. The mount functions only attach listeners: a ring is removed by its
 * `animationend` event, with a timer as a fallback when an animation is
 * cancelled. Reduced motion replaces the ring with a short background flash,
 * which is a plain element removed on a timer rather than an animation.
 */

export interface RippleGeometry {
  readonly diameter: number;
  readonly left: number;
  readonly top: number;
}

/** How far the ring grows past its base diameter. */
const RIPPLE_SCALE = 1.8;
const RIPPLE_DURATION = 600;
const FALLBACK_DELAY = RIPPLE_DURATION + 100;
const FLASH_DURATION = 120;

/**
 * A ring centred on `(x, y)` whose final radius reaches the farthest corner of
 * a `width` by `height` surface. The base diameter is divided by the scale so
 * the last frame covers the surface exactly.
 */
export function rippleGeometry(
  width: number,
  height: number,
  x: number,
  y: number,
): RippleGeometry {
  const reachX = Math.max(x, width - x);
  const reachY = Math.max(y, height - y);
  const reach = Math.hypot(reachX, reachY);
  return { diameter: (reach * 2) / RIPPLE_SCALE, left: x, top: y };
}

export function centreOf(width: number, height: number): { x: number; y: number } {
  return { x: width / 2, y: height / 2 };
}

function removeAfter(element: HTMLElement, delay: number): void {
  window.setTimeout(() => element.remove(), delay);
}

function spawnRing(layer: HTMLElement, box: DOMRect, x: number, y: number): void {
  const geometry = rippleGeometry(box.width, box.height, x, y);
  const ring = document.createElement('span');
  ring.className = 'ripple-ring';
  ring.setAttribute('aria-hidden', 'true');
  ring.style.left = `${geometry.left}px`;
  ring.style.top = `${geometry.top}px`;
  ring.style.setProperty('--ripple-diameter', `${geometry.diameter}px`);
  ring.addEventListener('animationend', () => ring.remove(), { once: true });
  layer.append(ring);
  removeAfter(ring, FALLBACK_DELAY);
}

function spawnFlash(layer: HTMLElement): void {
  const flash = document.createElement('span');
  flash.className = 'ripple-flash';
  flash.setAttribute('aria-hidden', 'true');
  layer.append(flash);
  removeAfter(flash, FLASH_DURATION);
}

function prefersReducedMotion(): boolean {
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

function spawn(layer: HTMLElement, box: DOMRect, x: number, y: number): void {
  if (prefersReducedMotion()) {
    spawnFlash(layer);
    return;
  }
  spawnRing(layer, box, x, y);
}

/** Attach the pointer and keyboard ripple to one surface. Idempotent. */
export function installRipple(surface: HTMLElement): void {
  if (surface.dataset.rippleReady === 'true') {
    return;
  }
  const layer = surface.querySelector<HTMLElement>('[data-ripple-layer]');
  if (layer === null) {
    return;
  }
  surface.dataset.rippleReady = 'true';

  surface.addEventListener('pointerdown', (event) => {
    if (event.button !== 0) {
      return;
    }
    const box = surface.getBoundingClientRect();
    spawn(layer, box, event.clientX - box.left, event.clientY - box.top);
  });

  surface.addEventListener('keydown', (event) => {
    if (event.repeat || (event.key !== 'Enter' && event.key !== ' ')) {
      return;
    }
    const target = event.target;
    if (
      !(target instanceof Element) ||
      target.closest('button, a, [role="button"], input, select, textarea') === null
    ) {
      return;
    }
    const box = surface.getBoundingClientRect();
    const centre = centreOf(box.width, box.height);
    spawn(layer, box, centre.x, centre.y);
  });
}

/** Attach every surface under `root`; safe to call more than once. */
export function installAllRipples(root: ParentNode = document): void {
  for (const surface of root.querySelectorAll<HTMLElement>('[data-ripple-surface]')) {
    installRipple(surface);
  }
}
