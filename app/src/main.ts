import { mount } from 'svelte';
import App from './App.svelte';
import { brand } from './lib/brand';
import { tourGate, tourMouse } from './lib/keys';
import './theme/fonts.css';
import './theme/tokens.css';
import './theme/base.css';

document.title = brand.displayName;
try {
  if (localStorage.getItem('cutaway.theme') === 'light') document.documentElement.dataset.theme = 'light';
} catch {
  /* storage blocked: dark */
}

// before anything mounts: a capture listener on the window runs in the order it was added
window.addEventListener('keydown', tourGate, true);
window.addEventListener('contextmenu', tourMouse, true);
window.addEventListener('dblclick', tourMouse, true);

const target = document.getElementById('app');
if (!target) throw new Error('missing #app');
export default mount(App, { target });
