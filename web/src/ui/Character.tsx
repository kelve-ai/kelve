// Where your agent's character stands. Every character is drawn on the same
// grid, 24 pixels wide and 27 tall, and moves the same way. Until kelve draws
// it from core's character, this is its outline: a plain robot in the page's
// greys, standing on the page, never in a box. `move` is how it moves: waiting
// (`idle`), working, pleased (`happy`), hello (`wave`) or asleep; each stops
// for anyone whose computer asks for less motion.

export type Move = 'idle' | 'work' | 'happy' | 'wave' | 'sleep'

/** The outline's pixels, as rectangles on the grid: x, y, width, height. */
const BODY: Array<[number, number, number, number]> = [
  [11, 0, 2, 3], // antenna
  [5, 3, 14, 10], // head
  [7, 14, 10, 8], // body
  [4, 15, 2, 5], // arms
  [18, 15, 2, 5],
]
const LEGS: Array<[number, number, number, number]> = [
  [8, 22, 3, 5],
  [13, 22, 3, 5],
]
const EYES: Array<[number, number, number, number]> = [
  [8, 7, 2, 2],
  [14, 7, 2, 2],
]

export function Character({ size = 120, move = 'idle', label = 'Your agent' }: {
  /** Its width in pixels on the page; its height follows the grid. */
  size?: number
  move?: Move
  label?: string
}) {
  const rect = ([x, y, w, h]: [number, number, number, number], i: number) => <rect key={i} x={x} y={y} width={w} height={h} />
  return (
    <span className={`char ${move}`} data-placeholder="" style={{ width: size }}>
      <svg role="img" aria-label={label} viewBox="0 0 24 27" width={size} height={(size * 27) / 24} shapeRendering="crispEdges">
        <g className="char-legs">{LEGS.map(rect)}</g>
        <g className="char-base">
          {BODY.map(rect)}
          <g className="char-eyes">{EYES.map(rect)}</g>
        </g>
      </svg>
      <i className="char-floor" aria-hidden="true" />
    </span>
  )
}
