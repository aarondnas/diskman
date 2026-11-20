<script lang="ts">
  // "Diskman" as pixel lettering that matches the 16 × 16 pixel logo (src-tauri/icons/icon.svg):
  // amber letters like the status lights with a hard pixel shadow like the cabinet. Each glyph is drawn on
  // a grid 7 pixels high; lowercase letters use the lower 5 rows, i's dot and k's stem the top 2.
  // size: screen pixels per grid pixel; with 2 it matches the logo shown at 32 px.
  let { size = 2 }: { size?: number } = $props();

  const GLYPHS: Record<string, string[]> = {
    D: ['####.', '#...#', '#...#', '#...#', '#...#', '#...#', '####.'],
    i: ['#', '.', '#', '#', '#', '#', '#'],
    s: ['....', '....', '.###', '#...', '.##.', '...#', '###.'],
    k: ['#...', '#...', '#..#', '#.#.', '##..', '#.#.', '#..#'],
    m: ['.....', '.....', '####.', '#.#.#', '#.#.#', '#.#.#', '#.#.#'],
    a: ['....', '....', '.##.', '...#', '.###', '#..#', '.###'],
    n: ['....', '....', '###.', '#..#', '#..#', '#..#', '#..#'],
  };
  const TEXT = 'Diskman';
  const GAP = 1;

  // Pixels as [x, y]; neighbouring pixels of a row are joined into one rect
  const runs: { x: number; y: number; w: number }[] = [];
  let x = 0;
  for (const ch of TEXT) {
    const rows = GLYPHS[ch];
    rows.forEach((row, y) => {
      let start = -1;
      for (let c = 0; c <= row.length; c++) {
        if (row[c] === '#' && start < 0) start = c;
        else if (row[c] !== '#' && start >= 0) {
          runs.push({ x: x + start, y, w: c - start });
          start = -1;
        }
      }
    });
    x += rows[0].length + GAP;
  }
  // +1 for the shadow, which sits one pixel to the right and below
  const width = x - GAP + 1;
  const height = 8;
</script>

<svg
  class="wordmark"
  viewBox="0 0 {width} {height}"
  width={width * size}
  height={height * size}
  shape-rendering="crispEdges"
  role="img"
  aria-label={TEXT}
>
  <g class="shadow">
    {#each runs as r}<rect x={r.x + 1} y={r.y + 1} width={r.w} height="1" />{/each}
  </g>
  <g class="face">
    {#each runs as r}<rect x={r.x} y={r.y} width={r.w} height="1" />{/each}
  </g>
</svg>

<style>
  .wordmark {
    display: block;
  }
  /* Cabinet color of the logo: a deep black, so the shadow still shows on the dark background;
     on the light theme the slate of the logo */
  .shadow {
    fill: #000000;
  }
  :global(:root[data-theme='light']) .shadow {
    fill: var(--on-amber);
  }
  .face {
    fill: var(--amber);
  }
</style>
