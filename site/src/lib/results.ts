/**
 * Evidence drawers and the condensed-response byte comparison.
 *
 * The page renders every fact into the server HTML, including the drawer
 * body, so a reader without JavaScript still has the evidence. This module
 * adds only the interaction: an evidence chip opens its native dialog, Tab
 * stays inside it, Escape closes it and returns focus to the chip, and the
 * ByteRace slider rewrites the two bars from the recorded byte counts.
 *
 * The byte arithmetic is pure and exported so the unit suite can check it
 * against the fact register without a browser.
 */

export interface ByteRaceModel {
  readonly fullBytes: number;
  readonly condensedBytes: number;
  readonly maxPaths: number;
  readonly paths: number;
  readonly bytes: number;
  readonly widthPercent: number;
}

/** A stable DOM id for one fact's evidence dialog. */
export function dialogIdFor(factId: string): string {
  return `evidence-${factId.toLowerCase().replace(/[^a-z0-9-]+/g, '-')}`;
}

/**
 * How many condensed responses fit inside one full response, at most. The
 * slider never promises a count the recorded bytes cannot support.
 */
export function maxCondensedPaths(fullBytes: number, condensedBytes: number): number {
  if (
    !Number.isFinite(fullBytes) ||
    !Number.isFinite(condensedBytes) ||
    fullBytes <= 0 ||
    condensedBytes <= 0
  ) {
    return 1;
  }
  return Math.max(1, Math.floor(fullBytes / condensedBytes));
}

/** Clamp a byte count to its share of a full response for the bar width. */
export function condensedWidthPercent(condensedBytes: number, fullBytes: number): number {
  if (!Number.isFinite(condensedBytes) || !Number.isFinite(fullBytes) || fullBytes <= 0) {
    return 0;
  }
  const percent = (condensedBytes / fullBytes) * 100;
  return Math.min(100, Math.max(0, percent));
}

/** Build one frame of the ByteRace display from the recorded facts. */
export function byteRaceModel(
  fullBytes: number,
  condensedBytes: number,
  paths: number,
): ByteRaceModel {
  const maxPaths = maxCondensedPaths(fullBytes, condensedBytes);
  const clamped = Number.isFinite(paths) ? Math.min(maxPaths, Math.max(1, Math.round(paths))) : 1;
  const bytes = clamped * condensedBytes;
  return {
    fullBytes,
    condensedBytes,
    maxPaths,
    paths: clamped,
    bytes,
    widthPercent: condensedWidthPercent(bytes, fullBytes),
  };
}

const byteFormat = new Intl.NumberFormat('en-US');

/** A whole byte count with thousands separators. */
export function formatByteCount(value: number): string {
  if (!Number.isFinite(value)) {
    return '0';
  }
  return byteFormat.format(Math.round(value));
}

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function focusableIn(container: HTMLElement): HTMLElement[] {
  return [...container.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (element) => element.getClientRects().length > 0,
  );
}

function copyText(text: string): Promise<void> {
  if (navigator.clipboard !== undefined) {
    return navigator.clipboard.writeText(text);
  }
  return Promise.resolve().then(() => {
    const field = document.createElement('textarea');
    field.value = text;
    field.setAttribute('readonly', '');
    field.style.position = 'fixed';
    field.style.opacity = '0';
    document.body.append(field);
    field.select();
    document.execCommand('copy');
    field.remove();
  });
}

function bindCopy(button: HTMLButtonElement, status: HTMLElement | null): void {
  const label = button.textContent ?? '';
  let timer = 0;
  button.addEventListener('click', () => {
    const pointer = button.dataset.copyPointer ?? '';
    void copyText(pointer)
      .then(() => {
        button.textContent = 'Copied';
        button.dataset.copied = 'true';
        if (status !== null) {
          status.textContent = `Copied ${pointer}`;
        }
        window.clearTimeout(timer);
        timer = window.setTimeout(() => {
          button.textContent = label;
          delete button.dataset.copied;
          if (status !== null) {
            status.textContent = '';
          }
        }, 1600);
      })
      .catch(() => {
        if (status !== null) {
          status.textContent = 'The clipboard refused the copy. Select the pointer and copy it.';
        }
      });
  });
}

function bindDrawer(dialog: HTMLDialogElement, opener: HTMLButtonElement): void {
  const close = dialog.querySelector<HTMLButtonElement>('[data-evidence-close]');
  const copy = dialog.querySelector<HTMLButtonElement>('[data-copy-pointer]');
  const status = dialog.querySelector<HTMLElement>('[data-copy-status]');

  close?.addEventListener('click', () => {
    dialog.close();
  });

  dialog.addEventListener('cancel', (event) => {
    event.preventDefault();
    dialog.close();
  });

  dialog.addEventListener('close', () => {
    opener.focus();
  });

  dialog.addEventListener('keydown', (event) => {
    if (event.key !== 'Tab') {
      return;
    }
    const focusable = focusableIn(dialog);
    if (focusable.length === 0) {
      event.preventDefault();
      return;
    }
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (first === undefined || last === undefined) {
      return;
    }
    const active = document.activeElement;
    if (event.shiftKey && (active === first || !dialog.contains(active))) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && (active === last || !dialog.contains(active))) {
      event.preventDefault();
      first.focus();
    }
  });

  if (copy !== null) {
    bindCopy(copy, status);
  }
}

function bindByteRace(race: HTMLElement): void {
  const fullBytes = Number(race.dataset.fullBytes);
  const condensedBytes = Number(race.dataset.condensedBytes);
  const range = race.querySelector<HTMLInputElement>('[data-byte-race-range]');
  const fill = race.querySelector<HTMLElement>('[data-byte-race-fill]');
  const counts = race.querySelectorAll<HTMLElement>('[data-byte-race-paths]');
  const byteLabels = race.querySelectorAll<HTMLElement>('[data-byte-race-bytes]');
  if (range === null) {
    return;
  }

  const first = byteRaceModel(fullBytes, condensedBytes, Number(range.value));
  range.max = String(first.maxPaths);

  const render = (paths: number): void => {
    const model = byteRaceModel(fullBytes, condensedBytes, paths);
    if (fill !== null) {
      fill.style.width = `${model.widthPercent}%`;
    }
    for (const count of counts) {
      count.textContent = String(model.paths);
    }
    for (const label of byteLabels) {
      label.textContent = formatByteCount(model.bytes);
    }
    range.value = String(model.paths);
    range.setAttribute(
      'aria-valuetext',
      `${model.paths} condensed responses, ${formatByteCount(model.bytes)} bytes`,
    );
  };

  range.addEventListener('input', () => {
    render(Number(range.value));
  });
  render(Number(range.value));
}

/**
 * Bind every evidence drawer and byte comparison inside `root`. Native
 * `<dialog>` supplies the modal layer; this function adds the explicit Tab
 * loop, Escape close, focus restoration and the byte arithmetic.
 */
export function initResults(root: Document | HTMLElement = document): void {
  const openers = new Map<HTMLDialogElement, HTMLButtonElement>();

  for (const button of root.querySelectorAll<HTMLButtonElement>('[data-evidence-open]')) {
    const id = button.dataset.evidenceOpen;
    if (id === undefined || id === '') {
      continue;
    }
    const dialog = root.querySelector<HTMLDialogElement>(`[data-evidence-drawer="${id}"]`);
    if (dialog === null) {
      continue;
    }
    openers.set(dialog, button);
    button.addEventListener('click', () => {
      if (!dialog.open) {
        dialog.showModal();
      }
    });
  }

  for (const dialog of root.querySelectorAll<HTMLDialogElement>('[data-evidence-drawer]')) {
    const opener = openers.get(dialog);
    if (opener !== undefined) {
      bindDrawer(dialog, opener);
    }
  }

  for (const race of root.querySelectorAll<HTMLElement>('[data-byte-race]')) {
    bindByteRace(race);
  }
}
