// kelve's tabs in this browser, and showing something in one. When kelve
// shows you something, it opens it in a new tab: only that brings your
// browser to the front with it shown, as no page can bring itself or another
// tab forward. When another kelve tab shows the same thing, the new one
// takes its place, where you were in it, and the old one closes. A browser
// lets a tab close itself only while it's been nowhere else; one you've
// moved around in says it's open in a newer tab instead.

/** The mark kelve's sign-in puts on a tab it opens to show something: `#/o/7?shown`. */
const SHOWN = 'shown'

/** How long after it opens a new tab moves to where you were in an old one: after that, you may have moved on in it. */
const SETTLING = 3000

type Said =
  /** A new tab asks which tabs show a thing. */
  | { kind: 'where'; opened: number; from: string }
  /** A tab showing it says where it is in it, and when it was last in sight. */
  | { kind: 'here'; opened: number; to: string; from: string; hash: string; seen: number }
  /** The new tab took their place: they go. */
  | { kind: 'taken'; opened: number; of: string[] }

const me = Math.random().toString(36).slice(2)
let moved = false
const movedListeners = new Set<() => void>()

/** What a tab shows, by the reader's address: `#/o/7?f=…` is 7. */
function showing(hash: string): number | null {
  const m = /^#\/o\/(\d+)(?:[/?]|$)/.exec(hash)
  return m ? Number(m[1]) : null
}

/** Whether this tab says it's open in a newer tab. */
export const isMoved = () => moved

export function onMoved(fn: () => void) {
  movedListeners.add(fn)
  return () => {
    movedListeners.delete(fn)
  }
}

function setMoved(to: boolean) {
  moved = to
  movedListeners.forEach((fn) => fn())
}

/** Shows what's open here again, after it moved to a newer tab. */
export const showHere = () => setMoved(false)

/**
 * Joins kelve's other tabs in this browser: says where this one is when a
 * new tab asks, and goes when one takes its place. A tab kelve just opened
 * to show something asks first, and takes the place of one showing it.
 */
export function joinTabs() {
  let channel: BroadcastChannel
  try {
    channel = new BroadcastChannel('kelve.tabs')
  } catch {
    return // No other tabs to hear: each shows what it's given, that's all.
  }
  const say = (said: Said) => channel.postMessage(said)
  let seen = document.hidden ? 0 : Date.now()
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) seen = Date.now()
  })

  // Opened by kelve to show something: the mark comes off, leaving this
  // tab's history a single entry, so it can go in turn.
  const [path, query = ''] = location.hash.split('?')
  const q = new URLSearchParams(query)
  let asking: { opened: number; since: number; seen: number } | null = null
  const opened = showing(location.hash)
  if (q.has(SHOWN) && opened !== null) {
    q.delete(SHOWN)
    history.replaceState(null, '', `${path}${q.size ? `?${q}` : ''}`)
    asking = { opened, since: Date.now(), seen: -1 }
  }

  channel.onmessage = (message: MessageEvent<Said>) => {
    const said = message.data
    const here = moved ? null : showing(location.hash)
    if (said.kind === 'where' && said.from !== me && said.opened === here)
      say({ kind: 'here', opened: here, to: said.from, from: me, hash: location.hash, seen: document.hidden ? seen : Date.now() })
    else if (said.kind === 'here' && said.to === me && asking?.opened === said.opened) {
      // Where you were in the one you saw last, while this tab is still settling.
      if (said.seen > asking.seen && Date.now() - asking.since < SETTLING && showing(location.hash) === said.opened) {
        asking.seen = said.seen
        if (said.hash !== location.hash) location.replace(said.hash)
      }
      say({ kind: 'taken', opened: said.opened, of: [said.from] })
    } else if (said.kind === 'taken' && said.of.includes(me) && said.opened === here) {
      window.close()
      // A tab you've moved around in stays open, and says where it went.
      if (!window.closed) setMoved(true)
    }
  }

  if (asking) say({ kind: 'where', opened: asking.opened, from: me })
}
