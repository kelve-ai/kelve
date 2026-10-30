// The states gallery, at #/states in development only: every state of every
// screen, drawn from the example with no server, light and dark side by side,
// at a phone's width on a toggle, with every string each shows listed under it,
// to read together.

import { useEffect, useRef, useState } from 'react'
import example from '../example/example.json'
import { states, type State } from '../example/states'
import { Choice } from '../ui/ui'

const DESKTOP = { width: 1440, height: 900 }
const PHONE = { width: 390, height: 844 }

/** One state, in one theme, drawn small. */
function Shot({ state, theme, phone, heard }: { state: State; theme: 'light' | 'dark'; phone: boolean; heard?: (words: string[]) => void }) {
  const frame = useRef<HTMLIFrameElement>(null)
  const size = phone ? PHONE : { ...DESKTOP, width: state.width ?? DESKTOP.width }
  const scale = phone ? 0.6 : 0.44
  const src = `/${state.path}${state.path.includes('?') ? '&' : '?'}theme=${theme}`
  useEffect(() => {
    const f = frame.current
    if (!f || (!state.clicks?.length && !state.points && !state.hovers)) return
    // Once it's drawn: what the state clicks, by the words it shows; and
    // the row it points at, drawn as pointed at.
    const click = () => {
      window.setTimeout(() => {
        for (const words of state.clicks ?? []) {
          const found = [...(f.contentDocument?.querySelectorAll<HTMLElement>('summary, button, label') ?? [])].find((el) => el.textContent?.trim() === words)
          found?.click()
        }
        if (state.points) {
          const row = [...(f.contentDocument?.querySelectorAll<HTMLElement>('.lrow') ?? [])].find((el) => el.textContent?.includes(state.points ?? ''))
          row?.classList.add('pointed')
        }
        if (state.hovers) f.contentDocument?.querySelector(state.hovers)?.classList.add('pointed')
      }, 1000)
    }
    f.addEventListener('load', click)
    return () => f.removeEventListener('load', click)
  }, [state.clicks, state.points, state.hovers, src])
  useEffect(() => {
    const f = frame.current
    if (!f || !heard) return
    const read = () => {
      // Once it's drawn: every line of words it shows.
      window.setTimeout(() => {
        const text = f.contentDocument?.body.innerText ?? ''
        heard([...new Set(text.split('\n').map((l) => l.trim()).filter((l) => l.length > 1))])
      }, 1500)
    }
    f.addEventListener('load', read)
    return () => f.removeEventListener('load', read)
  }, [heard, src])
  return (
    <div className="shot" style={{ width: size.width * scale, height: size.height * scale }}>
      <iframe ref={frame} title={`${state.title}, ${theme}`} src={src} style={{ width: size.width, height: size.height, transform: `scale(${scale})` }} />
    </div>
  )
}

function Row({ state, phone }: { state: State; phone: boolean }) {
  const [words, setWords] = useState<string[]>([])
  return (
    <section className="state" aria-label={state.title}>
      <h2>{state.title} <a href={state.path} target="_blank" rel="noreferrer"><code>{state.name}</code></a></h2>
      <div className="pair">
        <Shot state={state} theme="light" phone={phone} heard={setWords} />
        <Shot state={state} theme="dark" phone={phone} />
      </div>
      {words.length > 0 && (
        <details>
          <summary>Every string it shows ({words.length})</summary>
          <ul className="strings">{words.map((w) => <li key={w}>{w}</li>)}</ul>
        </details>
      )}
    </section>
  )
}

export function States() {
  const [phone, setPhone] = useState(false)
  const all = states(example.answers as Record<string, unknown>)
  const shown = phone ? all.filter((s) => s.phone) : all
  return (
    <main className="states">
      <header>
        <h1>Every state, from the example</h1>
        <p>Light and dark side by side. The screenshot tests compare each with how it looked (bun run looks).</p>
        <Choice label="Width" value={phone ? 'phone' : 'desktop'} onChange={(v) => setPhone(v === 'phone')} options={[{ value: 'desktop', label: 'Desktop' }, { value: 'phone', label: 'Phone' }]} />
      </header>
      {shown.map((s) => <Row key={`${s.name}:${phone}`} state={s} phone={phone} />)}
    </main>
  )
}
