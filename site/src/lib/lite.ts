/**
 * The one place the lite-mode decision lives.
 *
 * A reader gets the lighter page when they ask for it (`?lite=1`), when the
 * browser reports a metered connection (`saveData`), or when the device has
 * two gigabytes of memory or less. The answer is computed once.
 *
 * MS1 installs the hook only: the ambient components consult it, and the
 * motion module will return every effect's final state when it lands. The
 * switch is not advertised anywhere yet.
 */

interface ConnectionLike {
  readonly saveData?: boolean;
}

interface NavigatorLike {
  readonly connection?: ConnectionLike;
  readonly deviceMemory?: number;
}

let cached: boolean | null = null;

/** Whether this reader should get the light page. */
export function lite(): boolean {
  if (cached !== null) {
    return cached;
  }
  const navigatorLike = navigator as Navigator & NavigatorLike;
  let requested = false;
  try {
    requested = new URLSearchParams(window.location.search).get('lite') === '1';
  } catch {
    requested = false;
  }
  const saveData = navigatorLike.connection?.saveData === true;
  const memory = navigatorLike.deviceMemory;
  cached = requested || saveData || (typeof memory === 'number' && memory <= 2);
  return cached;
}

/** Forget the cached answer. Tests call this; the page never does. */
export function resetLite(): void {
  cached = null;
}
