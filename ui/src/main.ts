import { mount } from 'svelte';
import '../../themes.css';
import './app.css';
import App from './App.svelte';
import { applyTheme, storedTheme } from './lib/theme';

applyTheme(storedTheme());

const target = document.getElementById('app');
if (!target) {
  throw new Error('missing #app mount point');
}

const app = mount(App, { target });
export default app;
