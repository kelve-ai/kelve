// Your agent (board YourAgent): its character and name, with the brush to
// change how it looks; what it knows, a square for each part of each skill,
// coloured by what its checks found; Fits you, with the way to pick between
// two results (Fits.tsx); and its page on kelve.ai, or a verified score to get
// one (hosted/YourPage).

import type { PartCheck, YourAgentView } from '../../api/types'
import { EXAMPLE_AGENT } from '../../example/agent'
import { Loading } from '../../Loader'
import { hrefOf } from '../../routes'
import { Character } from '../../ui/Character'
import { Btn, TextLink } from '../../ui/kit'
import { madeOf } from '../../ui/pixels'
import { YourPage } from '../hosted/YourPage'
import { useAgentLoad } from './looks'

/** What each square says, read aloud and on hover. */
const SAYS: Record<PartCheck, string> = {
  works: 'Works',
  failed: 'Failed',
  waiting: 'Waiting for your agent',
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
  const page = useAgentLoad<YourAgentView>('/api/your-agent', example ? EXAMPLE_AGENT : undefined, ['settings', 'skills', 'asks'])
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
          <p>It keeps what you know as skills, and gets better with each piece of work.</p>
        </div>
      </section>

      <section className="ya-knows" aria-labelledby="knows">
        <h2 id="knows">What it knows</h2>
        {p.knows.length === 0 ? (
          <p className="ya-line">Nothing yet. It learns from each debrief you go through.</p>
        ) : (
          <>
            <p className="ya-line">A square for each part: green works, orange failed, violet is being checked, hollow violet is waiting for your agent, grey makes no difference, hollow isn’t checked yet.</p>
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

      {(p.fits || p.fitsOpen) && (
        <section className="ya-fits" aria-labelledby="fits">
          <div>
            <h2 id="fits">Fits you</h2>
            {p.fits ? (
              <p>You picked your agent in <strong>{p.fits.picked} of {p.fits.of}</strong> blind tests on your past work.</p>
            ) : (
              <p>Your agent redoes some of your past work twice: with your skills, and as a standard agent. You pick the one you’d rather keep, without knowing which is which.</p>
            )}
            <p className="ya-go">
              {p.fitsReady > 0 && <>{p.fitsReady === 1 ? 'A pick is waiting for you. ' : `${p.fitsReady} picks are waiting for you. `}</>}
              <TextLink href={hrefOf('agent/fits', example)}>Pick between two results</TextLink>
            </p>
          </div>
          {p.fits && (
            <div className="ya-tests" role="img" aria-label={`You picked your agent in ${p.fits.picked} of ${p.fits.of} blind tests`} style={{ gridTemplateColumns: `repeat(${p.fits.tests.length}, minmax(0, 1fr))` }}>
              {p.fits.tests.map((t, i) => <span key={i} className={t ? 'yes' : 'no'} />)}
            </div>
          )}
        </section>
      )}

      <YourPage example={example} />
    </div>
  )
}
