/**
 * Pure helpers for the browser dashboard window.
 *
 * The window itself is a thin DOM shell around the WebAssembly module; this
 * module owns the parts that can be reasoned about without a browser: the
 * build manifest the page fetches, the documents the module reads, the keys
 * the touch row presses and the keyboard maths for the "Run it locally" tabs.
 * Keeping them here lets the unit suite prove the contracts that the page and
 * the build integration both depend on, without a browser or a build.
 */

/** Where the page finds the names of the built module's files. */
export const TUI_MANIFEST_PATH = '/tui/manifest.json';

/** The origin paths the dashboard fetches at run time. */
export const TUI_DATA_DIR = '/data';

/**
 * The three documents the dashboard reads, by the names the site publishes.
 * The Rust loader owns the same paths; a test compares this list with
 * `crates/climb-tui` so the two cannot drift.
 */
export const TUI_DATA_FILES = [
  'climb-results.json',
  'ladder-verification.recorded.json',
  'dns-capture.recorded.json',
] as const;

/** One build of the browser module: its entry script and its wasm binary. */
export interface TuiManifest {
  readonly entry: string;
  readonly wasm: string;
}

/** The prefix every module asset is published under. */
const ASSET_PREFIX = '/tui/';

/** A flat, same-origin asset name under `/tui/`. */
const ASSET_NAME = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;

/**
 * Whether a value is a path this page may load: same origin, under `/tui/`,
 * a single path segment, no traversal and no query string. A manifest is read
 * from the network, so it is treated as untrusted input.
 */
export function isTuiAssetPath(value: unknown): value is string {
  if (typeof value !== 'string' || !value.startsWith(ASSET_PREFIX)) {
    return false;
  }
  const name = value.slice(ASSET_PREFIX.length);
  return ASSET_NAME.test(name) && !name.includes('..');
}

/**
 * Validate the JSON document the page fetches. Returns `null` rather than
 * throwing: a missing or malformed build is a stated state on the page, not a
 * crash.
 */
export function parseManifest(value: unknown): TuiManifest | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    return null;
  }
  const record = value as Record<string, unknown>;
  const { entry, wasm } = record;
  if (!isTuiAssetPath(entry) || !isTuiAssetPath(wasm)) {
    return null;
  }
  if (!entry.endsWith('.js') || !wasm.endsWith('_bg.wasm')) {
    return null;
  }
  return { entry, wasm };
}

/**
 * Pick the module's entry and binary out of a directory listing. Trunk writes
 * one hashed script beside one hashed `_bg.wasm`; anything else is ignored.
 */
export function tuiManifestFrom(files: readonly string[]): TuiManifest | null {
  const wasm = files.find((name) => name.endsWith('_bg.wasm'));
  const entry = files.find(
    (name) => name.endsWith('.js') && !name.includes('snippets') && !name.startsWith('.'),
  );
  if (wasm === undefined || entry === undefined) {
    return null;
  }
  return { entry: `${ASSET_PREFIX}${entry}`, wasm: `${ASSET_PREFIX}${wasm}` };
}

/** One button in the touch control row. */
export interface TouchControl {
  readonly id: string;
  readonly label: string;
  readonly hint: string;
  readonly key: string;
  readonly shift: boolean;
}

/**
 * The row a touch reader uses in place of a keyboard. Each control sends the
 * same DOM key the browser adapter already maps, so the row adds no second
 * vocabulary.
 */
export const TOUCH_CONTROLS: readonly TouchControl[] = [
  { id: 'previous', label: 'Previous view', hint: 'Previous', key: 'Tab', shift: true },
  { id: 'next', label: 'Next view', hint: 'Next', key: 'Tab', shift: false },
  { id: 'up', label: 'Up', hint: 'Up', key: 'ArrowUp', shift: false },
  { id: 'down', label: 'Down', hint: 'Down', key: 'ArrowDown', shift: false },
  { id: 'enter', label: 'Enter', hint: 'Enter', key: 'Enter', shift: false },
  { id: 'help', label: 'Show help', hint: 'Help', key: '?', shift: true },
  { id: 'theme', label: 'Cycle the theme', hint: 'Theme', key: 'T', shift: true },
  { id: 'ascii', label: 'Toggle ASCII glyphs', hint: 'ASCII', key: 'a', shift: false },
  { id: 'reload', label: 'Reload the data', hint: 'Reload', key: 'r', shift: false },
  { id: 'tour', label: 'Start the guided tour', hint: 'Tour', key: 't', shift: false },
];

/** The keyboard event a touch control sends to the dashboard. */
export interface TuiKeyEventInit {
  readonly key: string;
  readonly shiftKey: boolean;
  readonly bubbles: boolean;
  readonly cancelable: boolean;
}

/** The exact `keydown` a control dispatches, stated once so it can be tested. */
export function keyEventInit(control: TouchControl): TuiKeyEventInit {
  return {
    key: control.key,
    shiftKey: control.shift,
    bubbles: true,
    cancelable: true,
  };
}

/** The hint chip under the window. The keys are the dashboard's own. */
export const TUI_KEY_HINTS: readonly { readonly keys: string; readonly action: string }[] = [
  { keys: 'Tab', action: 'next view' },
  { keys: '1–9', action: 'jump to a view' },
  { keys: '?', action: 'help' },
  { keys: 'T', action: 'theme' },
  { keys: 't', action: 'tour' },
];

/**
 * The tab a vertical "Run it locally" tablist should activate for a key.
 * Returns `null` for a key the tablist does not claim, so the page can leave
 * native scrolling and browser shortcuts alone.
 */
export function nextTabIndex(current: number, key: string, count: number): number | null {
  if (count <= 0) {
    return null;
  }
  const active = current >= 0 && current < count ? current : 0;
  switch (key) {
    case 'ArrowRight':
    case 'ArrowDown':
      return (active + 1) % count;
    case 'ArrowLeft':
    case 'ArrowUp':
      return (active - 1 + count) % count;
    case 'Home':
      return 0;
    case 'End':
      return count - 1;
    default:
      return null;
  }
}
