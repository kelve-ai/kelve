// kelve's loader: its red square, fading in and out beside the words,
// wherever something is on its way (styles in app.css).
//   small:  beside a line of words
//   medium: a whole screen loading
// `still` is the square at rest, for a page that's waiting rather than
// loading. An entry being written shows its steps instead (Knowledge.tsx).

export function Loader({ size = 'medium', label = 'Loading', still = false }: { size?: 'medium' | 'small'; label?: string; still?: boolean }) {
  return <span className={`ld-sq ${size}${still ? ' still' : ''}`} role={still ? 'img' : 'status'} aria-label={label} />
}

/** A whole screen loading, or saying why it couldn't. */
export function Loading({ error, label = 'Loading' }: { error?: string; label?: string }) {
  if (error) return <div className="col" style={{ marginTop: 120 }}><p className="p">{error}</p></div>
  return <div className="loading"><Loader label={label} /></div>
}
