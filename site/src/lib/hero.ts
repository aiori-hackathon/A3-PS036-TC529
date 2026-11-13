/**
 * The hero chunk.
 *
 * The preloader imports this module dynamically, so a page without a hero
 * never downloads it. It resolves when every image inside a marked hero has
 * either decoded or failed; a failed image is not a reason to keep the
 * preloader on screen.
 */

export function whenHeroReady(root: ParentNode = document): Promise<void> {
  const images = [...root.querySelectorAll<HTMLImageElement>('[data-hero] img')];
  const pending = images.map((image) => {
    if (image.complete) {
      return Promise.resolve();
    }
    return new Promise<void>((resolve) => {
      image.addEventListener('load', () => resolve(), { once: true });
      image.addEventListener('error', () => resolve(), { once: true });
    });
  });
  return Promise.allSettled(pending).then(() => undefined);
}
