<script lang="ts" module>
  /** Inline stroke icons (24x24 grid), no icon font or network request. */
  const PATHS = {
    generate: 'M12 3l1.8 4.7L18.5 9.5l-4.7 1.8L12 16l-1.8-4.7L5.5 9.5l4.7-1.8z M19 15l.8 2.2L22 18l-2.2.8L19 21l-.8-2.2L16 18l2.2-.8z M5 15l.6 1.4L7 17l-1.4.6L5 19l-.6-1.4L3 17l1.4-.6z',
    gallery: 'M4 5h16v14H4z M4 15l4.5-4.5 4 4 2.5-2.5 5 5 M15.5 9.5a1.5 1.5 0 1 0 0-.01',
    connections: 'M9 7V3 M15 7V3 M7 7h10v4a5 5 0 0 1-10 0z M12 16v5',
    plus: 'M12 5v14 M5 12h14',
    minus: 'M5 12h14',
    close: 'M6 6l12 12 M18 6L6 18',
    check: 'M5 12.5l4.5 4.5L19 7.5',
    alert: 'M12 4l9 16H3z M12 10v4 M12 17.2v.1',
    info: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18z M12 11v6 M12 7.5v.1',
    chevronDown: 'M6 9l6 6 6-6',
    chevronRight: 'M9 6l6 6-6 6',
    chevronLeft: 'M15 6l-6 6 6 6',
    play: 'M8 5.5v13l10-6.5z',
    expand: 'M4 9V4h5 M20 9V4h-5 M4 15v5h5 M20 15v5h-5',
    shrink: 'M9 4v5H4 M15 4v5h5 M9 20v-5H4 M15 20v-5h5',
    history: 'M12 7v5l3 2 M4 12a8 8 0 1 0 2.3-5.6L4 8.5 M4 4v4.5h4.5',
    folder: 'M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2h8.5A1.5 1.5 0 0 1 21 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z',
    image: 'M4 5h16v14H4z M8.5 10a1.5 1.5 0 1 0 0-.01 M20 15l-5-5-9 9',
    refresh: 'M20 11a8 8 0 0 0-14.5-4.5L4 8 M4 4v4h4 M4 13a8 8 0 0 0 14.5 4.5L20 16 M20 20v-4h-4',
    stop: 'M7 7h10v10H7z',
    trash: 'M4 7h16 M9 7V4h6v3 M6 7l1 13h10l1-13',
    upload: 'M12 16V4 M7 9l5-5 5 5 M4 20h16',
    external: 'M14 4h6v6 M20 4l-9 9 M18 14v6H4V6h6',
    key: 'M8 11a4 4 0 1 0 0 8 4 4 0 0 0 0-8z M10.8 12.2L20 3 M16.5 6.5l2.5 2.5 M14 9l2 2',
    star: 'M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8-5.2-2.7-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z',
    lock: 'M6 11h12v9H6z M8.5 11V8a3.5 3.5 0 0 1 7 0v3',
    edit: 'M4 20h4L19 9l-4-4L4 16z M13.5 6.5l4 4',
    terminal: 'M4 5h16v14H4z M7.5 9.5l3 2.5-3 2.5 M12.5 15h4',
  } as const;

  export type IconName = keyof typeof PATHS;
</script>

<script lang="ts">
  interface Props {
    name: IconName;
    size?: number;
    /** Text for screen readers; without it the icon is decorative. */
    label?: string;
  }

  let { name, size = 18, label }: Props = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  role={label ? 'img' : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : 'true'}
>
  <path d={PATHS[name]} />
</svg>

<style>
  svg {
    flex: none;
    display: block;
  }
</style>
