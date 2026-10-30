// What went wrong, as kelve kept it: each report shown exactly as it is,
// to post as an issue (read again on GitHub before it goes) or to forget.
// kelve itself sends nothing.

import { del } from '../api/client'
import type { Problem } from '../api/types'
import { useLoad } from '../useLoad'
import { roughly } from '../words'

export function ProblemsSection() {
  const problems = useLoad<Problem[]>('/api/problems', ['settings'])
  const list = problems.data ?? []
  return (
    <section id="problems" className="setting" aria-label="When something goes wrong">
      <h2 className="h2">When something goes wrong</h2>
      <p className="what">
        kelve keeps a short report, with no secrets, no file names and nothing you opened, and sends it nowhere. You can read each one, and post it as an issue on kelve’s repository if you’d like it fixed.
      </p>
      {list.length === 0 ? (
        <p className="p" style={{ margin: 0 }}>Nothing has gone wrong.</p>
      ) : (
        <ul className="problems">
          {list.map((p) => (
            <li key={p.id}>
              <p className="p" style={{ margin: 0 }}>
                {p.what === 'the page' ? 'Something went wrong on the page' : 'kelve stopped unexpectedly'}, {roughly(p.at)}.
              </p>
              <details>
                <summary>See exactly what it says</summary>
                <pre className="report">{p.text}</pre>
              </details>
              <p className="p" style={{ margin: '6px 0 0' }}>
                <a className="word" href={p.issue} target="_blank" rel="noreferrer noopener">Report it on GitHub</a>, or{' '}
                <button className="word soft" onClick={() => void del(`/api/problems/${encodeURIComponent(p.id)}`).then(problems.reload)}>forget it</button>.
              </p>
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}

/** A line on the start page when there's a report to look at. */
export function ProblemsNotice() {
  const problems = useLoad<Problem[]>('/api/problems', ['settings'])
  if (!problems.data?.length) return null
  return (
    <p className="p soft">
      Something went wrong in kelve, and it kept a short report. <a className="link" href="#/you/shared">See it in your settings</a>.
    </p>
  )
}
