// Keyboard map (docs/specs/frontend-design.md). The box editor handles its own keys.
import { app } from './app.svelte';
import { sourceAt } from './lines';

/** A left or right arrow on a control that steps with them: a clip's trim edge. */
export function ownArrows(e: KeyboardEvent) {
  return (e.key === 'ArrowLeft' || e.key === 'ArrowRight') && e.target instanceof Element && e.target.hasAttribute('data-own-arrows');
}

const PAGE_KEYS = ['1', '2', '3', '4'];

export function onKey(e: KeyboardEvent) {
  const t = e.target as HTMLElement | null;
  // a slider keeps the focus after a drag: its undo must still work, its arrows stay its own
  const sliderUndo = t instanceof HTMLInputElement && t.type === 'range' && (e.metaKey || e.ctrlKey) && (e.key === 'z' || e.key === 'Z');
  const exportKey = (e.metaKey || e.ctrlKey) && (e.key === 'e' || e.key === 'E');
  if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT') && e.key !== 'Escape' && !sliderUndo && !exportKey) return;
  if (app.overlay === 'editor' || app.overlay === 'mapping') return;
  if (ownArrows(e)) return;
  // a button reached with Tab does what it says; after a click Space still plays
  if ((e.key === 'Enter' || e.key === ' ') && t?.tagName === 'BUTTON' && t.closest('[data-own-keys]') && t.matches(':focus-visible')) return;
  // Arrange's sounds layer sees keys first: it passes on whatever isn't about its sounds
  if (!app.overlay && app.page === 'arrange' && app.soundsUi?.key(e)) {
    e.preventDefault();
    e.stopPropagation();
    return;
  }
  const k = e.key;
  const mod = e.metaKey || e.ctrlKey;
  let handled = true;
  if (mod && (k === 'z' || k === 'Z')) {
    // what's still in a burst goes first, or the undo would take the step before it
    app.flushLineDepth();
    if (e.shiftKey) void app.redo();
    else void app.undo();
  } else if (mod && (k === 's' || k === 'S')) app.overlay = 'history';
  else if (exportKey) {
    if (app.page === 'export') app.exportUi?.run();
    else if (app.view) app.setPage('export');
  }
  else if (k === '?') app.overlay = app.overlay === 'help' ? null : 'help';
  else if (k === 'Escape') {
    if (app.overlay) app.overlay = null;
    else if (app.drawer) app.drawer = null;
    else if (app.lineSel.length) app.lineSel = [];
    else app.passageTo = null;
  } else if (app.overlay) handled = false;
  else if (app.page === 'import' && !(e.shiftKey && (k === 'b' || k === 'B')) && !PAGE_KEYS.includes(k)) handled = false;
  else if ((app.page === 'dev' || app.page === 'export') && !PAGE_KEYS.includes(k)) handled = false;
  // by the key's place: with ⌥ held, a Mac types ç and √ here
  else if (mod && e.altKey && e.code === 'KeyC' && app.page === 'arrange') app.copyAttrs(app.lines.find((l) => l.line_id === app.lineSel[app.lineSel.length - 1]) ?? app.cur);
  else if (mod && e.altKey && e.code === 'KeyV' && app.page === 'arrange') void app.pasteAttrs(app.pickedLines.map((l) => l.line_id));
  else if (k === ' ' && e.shiftKey) void app.playFromLocator();
  else if (k === ' ') void (app.preview?.playing ? app.stopSound() : app.togglePlay());
  else if (k === 'Enter') void app.playLineInContext();
  else if (k === 'ArrowUp' && e.altKey && app.page === 'arrange') void moveLine(-1);
  else if (k === 'ArrowDown' && e.altKey && app.page === 'arrange') void moveLine(1);
  else if (k === 'ArrowUp' && e.shiftKey) app.extendPassage(-1);
  else if (k === 'ArrowDown' && e.shiftKey) app.extendPassage(1);
  else if (k === 'ArrowUp') app.go(-1);
  else if (k === 'ArrowDown') app.go(1);
  else if ((k === 'ArrowLeft' || k === 'ArrowRight') && e.altKey && app.page === 'arrange') app.nudgeLineDepth(k === 'ArrowRight' ? 0.05 : -0.05);
  else if (k === 'ArrowLeft' && app.page !== 'edit') void app.stepTake(-1);
  else if (k === 'ArrowRight' && app.page !== 'edit') void app.stepTake(1);
  else if (e.ctrlKey && (k === 'm' || k === 'M')) void app.toggleMuteCurrent();
  else if ((k === 'm' || k === 'M') && !mod && app.page === 'options' && app.cur) app.openMapping({ kind: 'take', line_id: app.cur.line_id });
  else if (k === 'f' || k === 'F') void app.toggleFav();
  else if (k === 'e' || k === 'E') openEditor();
  else if (mod && (k === 'i' || k === 'I')) app.drawer = app.drawer === 'inspector' ? null : 'inspector';
  else if ((k === 'i' || k === 'I') && app.page !== 'edit') void setInOut('in');
  else if ((k === 'o' || k === 'O') && app.page !== 'edit') void setInOut('out');
  else if (k === 'p' || k === 'P') void app.applyPassage();
  else if (k === 'u' || k === 'U') app.drawer = app.drawer === 'unmatched' ? null : 'unmatched';
  else if ((k === 'b' || k === 'B') && e.shiftKey) app.toggleFullBucket();
  else if ((k === 'b' || k === 'B') && (app.page === 'edit' || (app.page === 'arrange' && app.soundsOn))) app.setBucket(!app.bucketOn);
  else if ((k === 's' || k === 'S') && app.page === 'arrange') app.toggleSounds();
  else if (app.page === 'import') handled = false;
  else if (k === 'b' || k === 'B') app.drawer = app.drawer === 'bucket' ? null : 'bucket';
  else if (k === '1' && app.view) app.setPage('options');
  else if (k === '2' && app.view) app.setPage('arrange');
  else if (k === '3' && app.view) app.setPage('edit');
  else if (k === '4' && app.view) app.setPage('export');
  else if (k === '+' || k === '=') app.scale = Math.min(1.3, Math.round((app.scale + 0.05) * 100) / 100);
  else if (k === '-' || k === '_') app.scale = Math.max(0.8, Math.round((app.scale - 0.05) * 100) / 100);
  else if (k === ',') void app.nudgeGap(-0.1);
  else if (k === '.') void app.nudgeGap(0.1);
  else handled = false;
  if (handled) {
    e.preventDefault();
    e.stopPropagation();
  }
}

export function openEditor() {
  const c = app.cur;
  if (!c || c.takes.length === 0) return;
  if (app.playing) void app.stop();
  app.overlay = 'editor';
}

async function moveLine(d: number) {
  const v = app.view;
  const c = app.cur;
  if (!v || !c) return;
  const idx = v.items.findIndex((i) => i.line_id === c.line_id);
  // move past the neighbouring line (and any spacer between)
  let j = idx + d;
  while (j >= 0 && j < v.items.length && v.items[j].kind !== 'line') j += d;
  if (j < 0 || j >= v.items.length) return;
  await app.cmd({ cmd: 'move_item', scene_id: v.scene.id, item_id: c.line_id, to_index: j });
}

async function setInOut(which: 'in' | 'out') {
  const c = app.cur;
  if (!c || c.take_index == null) return;
  const take = c.takes[c.take_index];
  const into = Math.max(0, Math.min(app.locator - c.start_s, c.end_s - c.start_s));
  // the take's own time at the locator: past a cut word it's further on than the scene's
  const at = (sourceAt(c, app.locator, take.source_id) ?? take.src_in + c.head_s + into) - take.src_in;
  if (which === 'in') {
    await app.cmd({ cmd: 'set_trim', line_id: c.line_id, head_s: at, tail_s: c.tail_s });
  } else {
    await app.cmd({ cmd: 'set_trim', line_id: c.line_id, head_s: c.head_s, tail_s: Math.max(0, take.duration - at) });
  }
}
