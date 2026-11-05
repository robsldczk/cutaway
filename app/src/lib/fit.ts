// A bar that never wraps: it steps down until it fits. Breakpoints can't do this, because how
// wide a bar is depends on the page, the Dev tab and the names in it, so it's measured. Each step
// is a `data-fit` value the bar's CSS answers to; all of it happens before the frame is painted.

/** The first level that doesn't overflow, or the last one when none fits. */
export function firstFit(levels: number, overflows: (level: number) => boolean): number {
  for (let l = 0; l < levels; l++) if (!overflows(l)) return l;
  return levels;
}

export interface FitOptions {
  /** Steps beyond 0. */
  levels: number;
  /** Measures again when this changes: what's in the bar, which its size alone doesn't show. */
  watch?: unknown;
}

export function fit(node: HTMLElement, options: FitOptions) {
  let { levels } = options;
  let frame = 0;
  const run = () => {
    const level = firstFit(levels, (l) => {
      node.dataset.fit = String(l);
      return node.scrollWidth > node.clientWidth + 1;
    });
    node.dataset.fit = String(level);
  };
  // its children too: a page with more tools changes them, not the bar. This fires between layout
  // and paint, so it's as early as a measure can safely be
  const seen = new ResizeObserver(run);
  seen.observe(node);
  for (const child of node.children) seen.observe(child);
  // the first measure may be in a fallback font
  void document.fonts?.ready.then(run);
  run();
  return {
    update(next: FitOptions) {
      levels = next.levels;
      // not now: measuring forces a style pass in the middle of Svelte's update, and whatever
      // else is changing there commits early and skips its transition (Arrange's column did).
      // The next frame is still before the paint.
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(run);
    },
    destroy: () => {
      cancelAnimationFrame(frame);
      seen.disconnect();
    },
  };
}
