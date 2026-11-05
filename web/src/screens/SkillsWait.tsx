// What the skills pages and Fits you show while what they show is on its
// way, or, when it couldn't be fetched, why, with a way back.

import { Loading } from '../Loader'
import { hrefOf } from '../routes'

export function Wait({ error, example }: { error?: string; example?: boolean }) {
  if (!error) return <Loading />
  return (
    <div className="sk-empty">
      <h1>That isn’t here</h1>
      <p>{error}</p>
      <a className="go" href={hrefOf('skills', example)}>Your skills</a>
    </div>
  )
}
