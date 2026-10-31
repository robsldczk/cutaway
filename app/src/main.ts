import { mount } from 'svelte';
import App from './App.svelte';
import { brand } from './lib/brand';
import './theme/fonts.css';
import './theme/tokens.css';

document.title = brand.displayName;

const target = document.getElementById('app');
if (!target) throw new Error('missing #app');
export default mount(App, { target });
