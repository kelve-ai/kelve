// How a character moves when it's left on the page for a while: not one
// move on a loop. Resting, it mostly stays still, with long, uneven pauses,
// and now and then blinks, looks about, shifts a little or waves. Working, it
// works in stretches, with a breath between them. For anyone whose computer
// asks for less motion, nothing here runs: it stays as it is, which is also
// how the screenshot tests see it.

import { useEffect, useState, useSyncExternalStore } from 'react'
import type { CharacterMove } from '../api/types'

/** A small thing a character does on top of a move, with its class on the drawing (shell.css). */
export type Beat = 'blink' | 'look-left' | 'look-right'

/** What it's doing now. */
export type Now = { move: CharacterMove; beat?: Beat }

/** A step of what it does: for how long. */
type Step = Now & { ms: number }

const between = (low: number, high: number) => low + Math.random() * (high - low)

/** How long it stays still: a good while, and sometimes much longer. */
const pause = () => between(4_000, 11_000) * (Math.random() < 0.3 ? 2.2 : 1)

const still = (ms: number): Step => ({ move: 'still', ms })
const blink = (): Step => ({ move: 'still', beat: 'blink', ms: 140 })
const look = (beat: Beat): Step => ({ move: 'still', beat, ms: between(900, 1_900) })

/** Something small, between pauses. A wave is rare, but for saying hello. */
function somethingSmall(hello: boolean): Step[] {
  const roll = Math.random()
  if (roll < 0.5) return Math.random() < 0.25 ? [blink(), still(180), blink()] : [blink()]
  if (roll < 0.75) {
    const first: Beat = Math.random() < 0.5 ? 'look-left' : 'look-right'
    const other: Beat = first === 'look-left' ? 'look-right' : 'look-left'
    return Math.random() < 0.3 ? [look(first), still(between(300, 700)), look(other)] : [look(first)]
  }
  if (roll < (hello ? 0.85 : 0.94)) return [{ move: 'idle', ms: Math.random() < 0.5 ? 3_200 : 4_400 }]
  return [{ move: 'wave', ms: Math.random() < 0.5 ? 1_600 : 2_400 }]
}

/** A stretch of what `base` does, then the next, for as long as it's shown. */
function next(base: CharacterMove): Step[] {
  if (base === 'work') {
    const breath = between(3_000, 8_000)
    return [{ move: 'work', ms: between(7_000, 20_000) }, ...(Math.random() < 0.5 ? [still(breath / 2), blink(), still(breath / 2)] : [still(breath)])]
  }
  return [still(pause()), ...somethingSmall(base === 'wave')]
}

/** What it does first: a move that says something is seen in full before it rests. */
function first(base: CharacterMove): Step[] {
  if (base === 'wave') return [{ move: 'wave', ms: 2_400 }]
  if (base === 'happy') return [{ move: 'happy', ms: 2_100 }]
  return []
}

const QUIET = '(prefers-reduced-motion: reduce)'

/** Whether this computer asks for less motion, as it changes. */
function useLessMotion(): boolean {
  return useSyncExternalStore(
    (changed) => {
      const media = window.matchMedia(QUIET)
      media.addEventListener('change', changed)
      return () => media.removeEventListener('change', changed)
    },
    () => window.matchMedia(QUIET).matches,
  )
}

/**
 * What a character doing `base` does right now, when `on`. Only waiting,
 * waving, working and being pleased vary: asleep and still stay as they are.
 */
export function useLively(base: CharacterMove, on: boolean): Now {
  const less = useLessMotion()
  const live = on && !less && base !== 'sleep' && base !== 'still'
  const [now, setNow] = useState<Now & { of: CharacterMove }>({ move: base, of: base })
  useEffect(() => {
    if (!live) return
    let timer = 0
    let steps = first(base)
    const go = () => {
      if (steps.length === 0) steps = next(base)
      const [step, ...rest] = steps
      steps = rest
      setNow({ move: step.move, beat: step.beat, of: base })
      timer = window.setTimeout(go, step.ms)
    }
    go()
    return () => window.clearTimeout(timer)
  }, [live, base])
  return live && now.of === base ? now : { move: base }
}
