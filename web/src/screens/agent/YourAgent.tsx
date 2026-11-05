// Your agent (board YourAgent), before signing in to kelve.ai: its character
// and name, with the brush to change how it looks; what it knows, a square
// for each part of each skill, coloured by what its checks found; Fits you;
// and a verified score, to give it a page.

import type { PartCheck, YourAgentView } from '../../api/types'
import { EXAMPLE_AGENT } from '../../example/agent'
import { Loading } from '../../Loader'
import { hrefOf } from '../../routes'
import { Character } from '../../ui/Character'
import { Btn } from '../../ui/kit'
import { madeOf } from '../../ui/pixels'
import { useAgentLoad } from './looks'

/** What each square says, read aloud and on hover. */
const SAYS: Record<PartCheck, string> = {
  works: 'Works',
  failed: 'Failed',
  checking: 'Checking now',
  noDifference: 'Makes no difference',
  notChecked: 'Not checked yet',
}

const BRUSH = (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M18.4 2.6a2 2 0 0 1 2.9 2.9L11 15.8 8 16l.2-3 10.2-10.4Z" />
    <path d="M7 17c-2 0-3.5 1.4-3.5 3.5H7a3 3 0 0 0 0-6" />
  </svg>
)

const CHEV = (
  <svg className="chev" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M9 6l6 6-6 6" />
  </svg>
)

export function YourAgent({ example }: { example?: boolean }) {
  const page = useAgentLoad<YourAgentView>('/api/your-agent', example ? EXAMPLE_AGENT : undefined, ['settings'])
  const p = page.data
  if (!p) return <Loading error={page.error} />
  const c = p.character
  return (
    <div className="you-agent">
      <section className="ya-top" aria-label="Your agent">
        <Character size={240} look={c.look} made={madeOf(c.made)} label={c.name} />
        <div className="ya-name">
          <div className="ya-title">
            <h1>{c.name}</h1>
            <Btn icon href={hrefOf('agent/look', example)} label="Change how it looks">{BRUSH}</Btn>
          </div>
          <p>It learns from what you correct, and keeps it as skills.</p>
        </div>
      </section>

      <section className="ya-knows" aria-labelledby="knows">
        <h2 id="knows">What it knows</h2>
        {p.knows.length === 0 ? (
          <p className="ya-line">Nothing yet. It learns as you correct it.</p>
        ) : (
          <>
            <p className="ya-line">A square for each part: green works, orange failed, violet is being checked, grey makes no difference, hollow isn’t checked yet.</p>
            <div className="ya-rows">
              {p.knows.map((k) => {
                const works = k.parts.filter((x) => x === 'works').length
                return (
                  <a key={k.name} className="ya-row" href={hrefOf(`skills/${encodeURIComponent(k.name)}`, example)} aria-label={`${k.title}, ${k.parts.length} ${k.parts.length === 1 ? 'part' : 'parts'}, ${works} ${works === 1 ? 'works' : 'work'}`}>
                    <span className={k.kind === 'project' ? 'mono' : undefined}>{k.title}</span>
                    <span className="psqs" aria-hidden="true">
                      {k.parts.map((x, i) => <i key={i} className={`psq ${x}`} title={SAYS[x]} />)}
                    </span>
                    {CHEV}
                  </a>
                )
              })}
            </div>
          </>
        )}
      </section>

      {p.fits && (
        <section className="ya-fits" aria-labelledby="fits">
          <div>
            <h2 id="fits">Fits you</h2>
            <p>You picked your agent in <strong>{p.fits.picked} of {p.fits.of}</strong> blind tests on your past work.</p>
          </div>
          <div className="ya-tests" role="img" aria-label={`You picked your agent in ${p.fits.picked} of ${p.fits.of} blind tests`} style={{ gridTemplateColumns: `repeat(${p.fits.tests.length}, minmax(0, 1fr))` }}>
            {p.fits.tests.map((t, i) => <span key={i} className={t ? 'yes' : 'no'} />)}
          </div>
        </section>
      )}

      <section className="ya-score" aria-label="Your page">
        <p>A verified score gives your agent a page to share.</p>
        <Btn kind="primary" size="large" href={hrefOf('agent/score/new', example)}>Get a verified score</Btn>
      </section>
    </div>
  )
}
