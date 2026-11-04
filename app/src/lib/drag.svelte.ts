// Dragging a sound across the window, with the mouse. HTML5 drag and drop can't do this in the
// desktop app: Tauri's file drop answers every drag that enters the webview, so dragover and
// drop never reach the page, while a browser shows nothing wrong (D83). Finder drops need
// that handler, so it stays on.
import type { DragSound, Preview } from './types';

export type Dragged = ({ kind: 'bucket' } & DragSound) | { kind: 'search'; name: string; preview: Preview };
type Kind = Dragged['kind'];
type Of<K extends Kind> = Extract<Dragged, { kind: K }>;

export interface DropZone<K extends Kind> {
  takes: K;
  /** The sound is over the zone. True when the zone draws it itself, in place of the pill. */
  over?: (e: MouseEvent, item: Of<K>) => boolean | void;
  leave?: () => void;
  drop: (e: MouseEvent, item: Of<K>) => void;
}

/** Past this a press on a row is a drag; short of it, a click. */
const START_PX = 4;

class DragState {
  item = $state<Dragged | null>(null);
  x = $state(0);
  y = $state(0);
  /** The zone under the pointer shows the sound. */
  shown = $state(false);
}

export const drag = new DragState();

const zones = new WeakMap<HTMLElement, DropZone<Kind>>();

function zoneAt(e: MouseEvent, item: Dragged): DropZone<Kind> | null {
  const el = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>('[data-dropzone]');
  const z = el ? zones.get(el) : undefined;
  return z && z.takes === item.kind ? z : null;
}

/** Marks an element as a place to drop on. `null` makes it take nothing. */
export function dropzone<K extends Kind>(node: HTMLElement, zone: DropZone<K> | null) {
  const set = (z: DropZone<K> | null) => {
    if (z) zones.set(node, z as unknown as DropZone<Kind>);
    else zones.delete(node);
    node.toggleAttribute('data-dropzone', !!z);
  };
  set(zone);
  return { update: set, destroy: () => zones.delete(node) };
}

/** Call from a row's mousedown. A press on a control inside the row stays that control's. */
export function grab(e: MouseEvent, item: Dragged, onstart?: () => void) {
  const control = (e.target as HTMLElement).closest('button, input');
  if (e.button !== 0 || (control && control !== e.currentTarget)) return;
  e.preventDefault();
  const { clientX: x0, clientY: y0 } = e;
  let zone: DropZone<Kind> | null = null;

  const leave = () => {
    zone?.leave?.();
    zone = null;
    drag.shown = false;
  };
  const end = () => {
    window.removeEventListener('mousemove', move);
    window.removeEventListener('mouseup', up);
    window.removeEventListener('keydown', key, true);
    window.removeEventListener('blur', cancel);
    document.body.classList.remove('sound-drag');
    leave();
    drag.item = null;
  };
  function cancel() {
    end();
  }
  function move(ev: MouseEvent) {
    if (!drag.item) {
      if (Math.hypot(ev.clientX - x0, ev.clientY - y0) < START_PX) return;
      drag.item = item;
      document.body.classList.add('sound-drag');
      onstart?.();
    }
    drag.x = ev.clientX;
    drag.y = ev.clientY;
    const at = zoneAt(ev, item);
    if (at !== zone) leave();
    zone = at;
    drag.shown = at?.over?.(ev, item) === true;
  }
  function up(ev: MouseEvent) {
    const dragged = !!drag.item;
    const at = dragged ? zoneAt(ev, item) : null;
    end();
    if (dragged) swallowClick();
    at?.drop(ev, item);
  }
  function key(ev: KeyboardEvent) {
    if (ev.key !== 'Escape') return;
    ev.preventDefault();
    ev.stopPropagation();
    cancel();
  }
  window.addEventListener('mousemove', move);
  window.addEventListener('mouseup', up);
  window.addEventListener('keydown', key, true);
  window.addEventListener('blur', cancel);
}

/** The release of a drag isn't a click on whatever it ended over. */
function swallowClick() {
  const stop = (e: MouseEvent) => e.stopPropagation();
  window.addEventListener('click', stop, { capture: true, once: true });
  setTimeout(() => window.removeEventListener('click', stop, { capture: true }));
}
