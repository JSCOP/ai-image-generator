import { mount } from 'svelte';
import './lib/theme.css';
import App from './App.svelte';

async function start() {
  const target = document.getElementById('app');
  if (!target) throw new Error('#app element is missing');
  // Dev server only: ?mock=1 (or ?mock=empty) swaps the Rust backend for dev/mock-ipc.ts.
  // import.meta.env.DEV is false in `vite build`, so the import and its images are dropped from production bundles.
  if (import.meta.env.DEV) {
    const mode = new URLSearchParams(window.location.search).get('mock');
    if (mode !== null) {
      const { installMock } = await import('./dev/mock-ipc');
      installMock(mode);
    }
  }
  return mount(App, { target });
}

export default start();
