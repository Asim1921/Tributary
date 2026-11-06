import { mount } from 'svelte';

import App from './App.svelte';
import DropTarget from './DropTarget.svelte';
import './app.css';

const target = document.getElementById('app');
if (target) mount(location.hash === '#drop' ? DropTarget : App, { target });
