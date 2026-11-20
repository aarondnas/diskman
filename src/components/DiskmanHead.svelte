<script lang="ts">
  import { DEFAULT_VISOR } from '../utils/visor.ts';

  // The server rack of the app logo (src-tauri/icons/icon.svg) as 16 x 16 pixel art, with the
  // status lights in any colour (`visor`, the backup's colour): it marks a backup on its card
  // and in its editor. Sizes that are a multiple of 16 keep the pixels sharp. The rack fills all
  // 16 rows (feet two pixels high), so it sits centred in its box.
  let { visor = DEFAULT_VISOR, size = 32 }: { visor?: string; size?: number } = $props();
</script>

<svg viewBox="0 0 16 16" width={size} height={size} shape-rendering="crispEdges" aria-hidden="true">
  <!-- Cabinet and drive slots -->
  <path
    fill="#2b3440"
    d="M2 0h12v1H2zM1 1h14v1H1zM1 2h2v11H1zM13 2h2v11H13zM4 3h3v1H4zM3 5h10v1H3zM4 7h3v1H4zM3 9h10v1H3zM4 11h3v1H4zM1 13h14v1H1zM2 14h2v2H2zM12 14h2v2H12z"
  />
  <!-- Unit fronts -->
  <path
    fill="#8d97a3"
    d="M3 2h10v1H3zM3 3h1v1H3zM7 3h1v1H7zM12 3h1v1H12zM3 4h10v1H3zM3 6h10v1H3zM3 7h1v1H3zM7 7h1v1H7zM12 7h1v1H12zM3 8h10v1H3zM3 10h10v1H3zM3 11h1v1H3zM7 11h1v1H7zM12 11h1v1H12zM3 12h10v1H3z"
  />
  <!-- Status lights -->
  <path fill={visor} d="M8 3h4v1H8zM8 7h4v1H8zM8 11h4v1H8z" />
</svg>

<style>
  svg {
    flex: none;
    display: block;
  }
</style>
