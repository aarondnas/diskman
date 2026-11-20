import { mount } from 'svelte';
import './styles/global.css';
import App from './App.svelte';
import { applyTheme, storedLightMode } from './utils/theme.ts';

// Before the first paint, so the window opens in the theme of the last session
applyTheme(storedLightMode());

export default mount(App, { target: document.getElementById('app')! });
