// kelve's screens, by their address after `#/`:
//   (nothing)                   home
//   debrief/<id>                a debrief
//   change/<id>                 a change, file by file
//   skills, skills/<name>       your skills, and one of them
//   skills/<name>/edit          editing one
//   agent                       your agent
//   agent/look                  changing how it looks
//   agent/address               choosing your page's address on kelve.ai
//   agent/score/new             getting a verified score
//   agent/score/<id>            a verified score
//   settings/<section>          settings: agent, account, checks, reading, privacy, about
//   welcome                     meeting your agent, the first time
// The reader's (#/o/<id>) and today's settings (#/you/…) are App's own.

export const SECTIONS = ['agent', 'account', 'checks', 'reading', 'privacy', 'about'] as const
export type Section = (typeof SECTIONS)[number]

export type Route =
  | { screen: 'home' }
  | { screen: 'debrief'; id: string }
  | { screen: 'change'; id: string }
  | { screen: 'skills' }
  | { screen: 'skill'; name: string }
  | { screen: 'skill-edit'; name: string }
  | { screen: 'agent' }
  | { screen: 'look' }
  | { screen: 'address' }
  | { screen: 'score-new' }
  | { screen: 'score'; id: string }
  | { screen: 'settings'; section: Section }
  | { screen: 'welcome' }

/** The screen an address's parts name, or none: `home` is the example's own name for home. */
export function routeOf(parts: string[]): Route | null {
  const [a = '', b, c, d] = parts
  const none = (...rest: Array<string | undefined>) => rest.every((p) => p === undefined || p === '')
  if ((a === '' || a === 'home') && none(b)) return { screen: 'home' }
  if (a === 'debrief' && b && none(c)) return { screen: 'debrief', id: b }
  if (a === 'change' && b && none(c)) return { screen: 'change', id: b }
  if (a === 'skills' && none(b)) return { screen: 'skills' }
  if (a === 'skills' && b && none(c)) return { screen: 'skill', name: b }
  if (a === 'skills' && b && c === 'edit' && none(d)) return { screen: 'skill-edit', name: b }
  if (a === 'agent' && none(b)) return { screen: 'agent' }
  if (a === 'agent' && b === 'look' && none(c)) return { screen: 'look' }
  if (a === 'agent' && b === 'address' && none(c)) return { screen: 'address' }
  if (a === 'agent' && b === 'score' && c === 'new' && none(d)) return { screen: 'score-new' }
  if (a === 'agent' && b === 'score' && c && none(d)) return { screen: 'score', id: c }
  if (a === 'settings' && none(b)) return { screen: 'settings', section: 'agent' }
  if (a === 'settings' && SECTIONS.includes(b as Section) && none(c)) return { screen: 'settings', section: b as Section }
  if (a === 'welcome' && none(b)) return { screen: 'welcome' }
  return null
}

/**
 * Where a path after `#/` is: in the app, or in its example, which answers
 * with no server and whose home is `#/example/home`.
 */
export function hrefOf(path: string, example?: boolean): string {
  if (!example) return `#/${path}`
  return `#/example/${path || 'home'}`
}

/** Which of the header's places a screen is under. */
export function placeOf(route: Route): 'home' | 'skills' | 'agent' | 'settings' | null {
  switch (route.screen) {
    case 'home':
    case 'debrief':
    case 'change':
      return 'home'
    case 'skills':
    case 'skill':
    case 'skill-edit':
      return 'skills'
    case 'agent':
    case 'look':
    case 'address':
    case 'score-new':
    case 'score':
      return 'agent'
    case 'settings':
      return 'settings'
    case 'welcome':
      return null
  }
}
