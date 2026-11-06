// A file on its way in from the Finder. The webview sees no drag of its own here, only what
// Tauri reports, so a zone hit-tests the position it gives. On macOS that "physical" position
// is really in points, so it's used as CSS pixels.

class FileDrag {
  /** Audio on its way in; empty when nothing is. */
  paths = $state<string[]>([]);
  /** Everything on its way in, whatever it is: the welcome screen takes more than audio. */
  all = $state<string[]>([]);
  /** The zone under the pointer. */
  over = $state<string | null>(null);
}

export const fileDrag = new FileDrag();

export const AUDIO = /\.(wav|bwf|aif|aiff|mp3|flac|m4a)$/i;

/** The room's ambience, on Import › Rooms. */
export const AMBIENCE_ZONE = 'ambience';

export const zoneAt = (x: number, y: number) => document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-file-zone]')?.dataset.fileZone ?? null;

export function fileEnter(paths: string[]) {
  fileDrag.all = paths;
  fileDrag.paths = paths.filter((p) => AUDIO.test(p));
}

export function fileOver(x: number, y: number) {
  if (fileDrag.all.length) fileDrag.over = zoneAt(x, y);
}

export function fileEnd() {
  fileDrag.all = [];
  fileDrag.paths = [];
  fileDrag.over = null;
}
