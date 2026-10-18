// Gallery screen state (Svelte 5 runes) at module level, so filters and the selection stay when the user switches screens.

import * as api from './api';
import { gen } from './generate.svelte';
import { samePath } from './generateForm';
import { app, toast, toastError } from './state.svelte';
import type { FolderView, GalleryItem, ServiceError } from './types';
import type { StatusFilter } from './galleryView';

/** Records rendered at first and added per "더 보기". */
export const PAGE = 240;

export const gal = $state({
  items: [] as GalleryItem[],
  folders: [] as FolderView[],
  truncated: false,
  loaded: false,
  loading: false,
  error: null as ServiceError | null,
  /** Gallery root or null (every folder). */
  folder: null as string | null,
  status: 'all' as StatusFilter,
  query: '',
  /** recordPath of the selected record. */
  selected: null as string | null,
  shown: PAGE,
  /** A folder add/remove is running. */
  busy: false,
});

let request = 0;

/** Reads every registered folder again; the generate screen's history gets the same list. */
export async function loadGallery(): Promise<void> {
  const mine = ++request;
  gal.loading = true;
  try {
    const view = await api.gallery();
    if (mine !== request) return;
    gal.items = view.items;
    gal.folders = view.folders;
    gal.truncated = view.truncated;
    gal.error = null;
    gal.loaded = true;
    gen.history = view.items;
    gen.folders = view.folders;
    if (gal.folder !== null && !view.folders.some((f) => samePath(f.galleryRoot, gal.folder as string))) gal.folder = null;
    if (gal.selected !== null && !view.items.some((i) => i.recordPath === gal.selected)) gal.selected = null;
  } catch (e) {
    if (mine !== request) return;
    gal.error = api.toServiceError(e);
  } finally {
    if (mine === request) gal.loading = false;
  }
}

/** Registers a picked folder (its images show after the reload). */
export async function addFolder(path: string): Promise<void> {
  gal.busy = true;
  try {
    app.settings = await api.addGalleryFolder(path);
    await loadGallery();
    const added = gal.folders.find((f) => samePath(f.path, path));
    if (added) gal.folder = added.galleryRoot;
    toast('폴더를 갤러리에 추가했습니다.', 'ok');
  } catch (e) {
    toastError(api.toServiceError(e));
  } finally {
    gal.busy = false;
  }
}

/** Takes a folder out of the gallery; no file is deleted. */
export async function removeFolder(folder: FolderView): Promise<void> {
  gal.busy = true;
  try {
    app.settings = await api.removeGalleryFolder(folder.path);
    if (gal.folder !== null && samePath(gal.folder, folder.galleryRoot)) gal.folder = null;
    await loadGallery();
    toast('폴더를 갤러리에서 뺐습니다. 파일은 그대로 있습니다.', 'info');
  } catch (e) {
    toastError(api.toServiceError(e));
  } finally {
    gal.busy = false;
  }
}

/** A filter changed: start again from the first page. */
export function resetPage(): void {
  gal.shown = PAGE;
}
