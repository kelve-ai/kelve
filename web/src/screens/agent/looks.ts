// The looks your agent can start with, the moves it shows, and loading what
// the Your agent screens show: from kelve, or, in the example, Sam's.

import type { AgentLook, PartSlot, ServerEvent } from '../../api/types'
import { useLoad } from '../../useLoad'
import { defaultLook, library } from '../../ui/pixels'
import '../../styles/agent.css'

/** A look from a few parts and colours, the rest the robot's own (board Onboarding). */
function preset(parts: Partial<Record<PartSlot, string>>, colours: [string, string, string, string], background: string): AgentLook {
  const look = defaultLook()
  Object.assign(look.robot.parts, parts)
  const [body, light, screen, eyes] = colours
  look.robot.colours = { body, light, screen, eyes }
  look.background = background
  return look
}

/** The three looks to start with. */
export const STARTS: Array<{ key: string; label: string; look: AgentLook }> = [
  { key: 'sky', label: 'Sky', look: defaultLook() },
  {
    key: 'sunny',
    label: 'Sunny',
    look: preset(
      { head: 'box', hat: 'beanie', eyes: 'dots', mouth: 'flat', ears: 'cups', body: 'round', chest: 'heart', hands: 'claws', legs: 'wheels' },
      ['#ffd166', '#7c6cf2', '#1d1c1a', '#ffffff'],
      '#fff1d6',
    ),
  },
  {
    key: 'mint',
    label: 'Mint',
    look: preset(
      { head: 'bucket', antenna: 'double', eyes: 'visor', mouth: 'none', ears: 'none', body: 'slim', legs: 'hover' },
      ['#b8e0a8', '#2ec4b6', '#123b3a', '#ffd166'],
      '#e4f4df',
    ),
  },
]

/** Whether the person met their agent in this tab: home needn't wait for settings to say so. */
export const met = { now: false }

/** The moves, as people read them: Waiting, Working, Pleased, Hello, Asleep. */
export const MOVES = library.moves

/** Whether two looks are the same. */
export const sameLook = (a: AgentLook, b: AgentLook) => JSON.stringify(a) === JSON.stringify(b)

/**
 * What a Your agent screen shows: kelve's answer, fetched again as `screens`
 * change, or in the example, `example`, which never changes.
 */
export function useAgentLoad<T>(path: string, example: T | undefined, screens: ServerEvent['type'][] = ['settings']) {
  const loaded = useLoad<T>(example === undefined ? path : null, screens)
  return example === undefined ? loaded : { data: example, error: undefined, fresh: true, reload: () => {} }
}
