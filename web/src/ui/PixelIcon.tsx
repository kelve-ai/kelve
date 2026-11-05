// Pixel icons, 8 by 8, drawn like the character (board Home): one for each
// kind of thing on home, on a soft tile in its colour. Never read aloud: the
// words beside each say what it is.

export type Pixel = 'debrief' | 'check' | 'skill' | 'score' | 'part' | 'small' | 'done' | 'hook' | 'stalled' | 'failed'

const ART: Record<Pixel, string[]> = {
  debrief: ['.######.', '.#....#.', '.#.##.#.', '.#....#.', '.#.##.#.', '.#....#.', '.######.', '........'],
  check: ['..####..', '...##...', '...##...', '..#..#..', '.#....#.', '#..##..#', '#.####.#', '.######.'],
  skill: ['...##...', '...##...', '...##...', '########', '########', '...##...', '...##...', '...##...'],
  score: ['...##...', '...##...', '########', '.######.', '..####..', '.######.', '.##..##.', '##....##'],
  part: ['...#....', '...##...', '..###...', '..####..', '.#####..', '.######.', '########', '........'],
  small: ['........', '........', '..####..', '..#..#..', '..#..#..', '..####..', '........', '........'],
  done: ['........', '......##', '.....##.', '##..##..', '.####...', '..##....', '........', '........'],
  hook: ['..#..#..', '..#..#..', '.######.', '.######.', '..####..', '...##...', '...##...', '...##...'],
  stalled: ['........', '.##..##.', '.##..##.', '.##..##.', '.##..##.', '.##..##.', '.##..##.', '........'],
  failed: ['........', '.##..##.', '.##..##.', '.##..##.', '.##..##.', '.##..##.', '.##..##.', '........'],
}

/** Each pixel of a picture as a path's square. */
const path = (rows: string[]) =>
  rows.map((r, y) => [...r].map((c, x) => (c === '#' ? `M${x} ${y}h1v1h-1z` : '')).join('')).join('')

const PATHS = Object.fromEntries(Object.entries(ART).map(([k, rows]) => [k, path(rows)])) as Record<Pixel, string>

/** A kind's pixel icon on its tile. */
export function PixelIcon({ kind }: { kind: Pixel }) {
  return (
    <span className={`px-tile px-${kind}`} aria-hidden="true">
      <svg width="20" height="20" viewBox="0 0 8 8" shapeRendering="crispEdges">
        <path d={PATHS[kind]} />
      </svg>
    </span>
  )
}
