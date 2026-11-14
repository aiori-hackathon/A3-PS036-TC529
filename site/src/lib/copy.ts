/**
 * Clipboard helper shared by the copy buttons.
 *
 * The caller supplies the clipboard object so the unit test can drive the
 * success and refusal paths without a browser. A refused write returns false
 * and the component leaves the text selectable instead of claiming success.
 */

export interface ClipboardLike {
  writeText(text: string): Promise<void>;
}

/** Copy `text`, returning whether the platform accepted the write. */
export async function writeClipboard(
  text: string,
  clipboard: ClipboardLike | undefined,
): Promise<boolean> {
  if (clipboard === undefined) {
    return false;
  }
  try {
    await clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
