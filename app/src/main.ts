import { mount } from 'svelte';
import App from './App.svelte';
import { brand } from './lib/brand';
import './theme/fonts.css';
import './theme/tokens.css';
import './theme/base.css';

document.title = brand.displayName;
try {
  if (localStorage.getItem('cutaway.theme') === 'light') document.documentElement.dataset.theme = 'light';
} catch {
  /* storage blocked: dark */
}

const target = document.getElementById('app');
if (!target) throw new Error('missing #app');
export default mount(App, { target });
