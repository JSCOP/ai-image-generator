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
  /** "이 PC에서 찾기" is running. */
  discovering: false,
  /** "다른 PC" dialog. */
  shareOpen: false,
});

let searchedOnce = false;

let request = 0;

/** Reads every registered folder again; the generate screen's history gets the same list. */
export async function loadGallery(): Promise<void> {
  const mine = ++request;
  gal.loading = true;
  try {
    const view = await api.gallery();
    if (mine !== request) return;
    lastScanned = view.scanned ?? true;
    gal.items = view.items;
    gal.folders = view.folders;
    gal.truncated = view.truncated;
    gal.error = null;
    gal.loaded = true;
    // The generate screen works on this PC's records only.
    gen.history = view.items.filter((i) => !i.machine);
    gen.folders = view.folders.filter((f) => f.source !== 'peer');
    if (gal.folder !== null && !view.folders.some((f) => samePath(f.galleryRoot, gal.folder as string))) gal.folder = null;
    if (gal.selected !== null && !view.items.some((i) => i.recordPath === gal.selected)) gal.selected = null;
  } catch (e) {
    if (mine !== request) return;
    gal.error = api.toServiceError(e);
  } finally {
    if (mine === request) gal.loading = false;
  }
  // The first time ever, look for galleries made by the CLI or agents elsewhere on this PC.
  if (gal.loaded && !gal.error && !searchedOnce && !lastScanned) {
    searchedOnce = true;
    void discover(true);
  }
}

let lastScanned = true;

/** "이 PC에서 찾기": every ImageGallery folder on this PC's drives joins the gallery. */
export async function discover(quiet = false): Promise<void> {
  if (gal.discovering) return;
  gal.discovering = true;
  try {
    const found = await api.discoverGalleries();
    await loadGallery();
    if (!quiet || found > 0) toast(`이 PC에서 갤러리 폴더 ${found}개를 찾았습니다.`, 'ok');
  } catch (e) {
    toastError(api.toServiceError(e));
  } finally {
    gal.discovering = false;
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
