import { mount } from 'svelte';

import App from './App.svelte';
import DropTarget from './DropTarget.svelte';
import './app.css';
import { appLanguage, onLanguageChanged } from './lib/api';
import { useLanguage } from './lib/locale.svelte';

// Both windows speak the app's language from the start, and change with it.
useLanguage(await appLanguage().catch(() => 'en'));
void onLanguageChanged(useLanguage);

const target = document.getElementById('app');
if (target) mount(location.hash === '#drop' ? DropTarget : App, { target });
