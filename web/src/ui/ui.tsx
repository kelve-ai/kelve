// kelve's controls (board C-1): raised, never black. The one main action on a
// page is a raised white button; everything else beside it is a plain link. A
// choice of a few is a grey track with the chosen one raised. Something to copy
// sits in a light well with a Copy button. A hint is shown once, with an arrow,
// beside what it explains.

import { type ReactNode, useEffect, useState } from 'react'

export function Button({ children, onClick, disabled, type = 'button', small }: {
  children: ReactNode
  onClick?: () => void
  disabled?: boolean
  type?: 'button' | 'submit'
  small?: boolean
}) {
  return (
    <button type={type} className={`raised${small ? ' small' : ''}`} onClick={onClick} disabled={disabled}>
      {children}
    </button>
  )
}

/** A plain grey link beside the main action: "Not now", "Cancel". */
export function Quiet({ children, onClick }: { children: ReactNode; onClick?: () => void }) {
  return (
    <button type="button" className="quiet-link" onClick={onClick}>
      {children}
    </button>
  )
}

/** A choice of a few, on a grey track: the chosen one raised. */
export function Choice<T extends string>({ options, value, onChange, label, small }: {
  /** `dot`, when there is one: what the green dot by an option means, as its title. */
  options: Array<{ value: T; label: ReactNode; dot?: string }>
  value: T
  onChange: (value: T) => void
  label: string
  small?: boolean
}) {
  return (
    <span className={`track${small ? ' small' : ''}`} role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          className={o.value === value ? 'on' : undefined}
          title={o.dot}
          onClick={() => onChange(o.value)}
        >
          {o.dot && <i className="found" aria-hidden="true" />}
          {o.label}
        </button>
      ))}
    </span>
  )
}

const COPY_ICON = (
  <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
    <rect x="3.5" y="3.5" width="7" height="7" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1.2" />
    <path d="M8.5 2.5V2A1 1 0 0 0 7.5 1h-5A1 1 0 0 0 1.5 2v5a1 1 0 0 0 1 1H3" fill="none" stroke="currentColor" strokeWidth="1.2" />
  </svg>
)

/**
 * Something to copy: a command, with a `$` before it, or words to say to
 * an agent, without one. Copy says Copied for a moment.
 */
export function CopyWell({ text, words }: { text: string; words?: boolean }) {
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    if (!copied) return
    const t = window.setTimeout(() => setCopied(false), 1600)
    return () => window.clearTimeout(t)
  }, [copied])
  const copy = () => {
    void navigator.clipboard?.writeText(text).then(() => setCopied(true), () => {})
  }
  return (
    <div className={`well${words ? ' words' : ''}`}>
      <span className="what">{text}</span>
      <button type="button" className="raised small" onClick={copy} aria-label={copied ? 'Copied' : `Copy ${words ? 'the words' : 'the command'}`}>
        {COPY_ICON}
        {copied ? 'Copied' : 'Copy'}
      </button>
    </div>
  )
}

/** An on or off switch, with its label read aloud. */
export function Switch({ on, onChange, label }: { on: boolean; onChange: (on: boolean) => void; label: string }) {
  // On or off, as any other choice: plain words, the chosen one softly filled.
  return (
    <Choice small label={label} value={on ? 'on' : 'off'} onChange={(v) => onChange(v === 'on')} options={[{ value: 'on', label: 'On' }, { value: 'off', label: 'Off' }]} />
  )
}

/** Where something stands, as kelve's square: waiting (hollow), being done (red), done (filled); a dash for what's no one's work. */
/**
 * A mark on a row (boards Marks-1 to Marks-3): a square for something to
 * read, filled while it waits for you (`ready`) and hollow once read (`read`); a dial for your agent's work,
 * waiting (`queued`), at work (`now`) or done (`done`).
 */
export function State({ state, label }: { state: 'ready' | 'read' | 'now' | 'dash' | 'queued' | 'done'; label?: string }) {
  return <span className={`lsq ${state === 'read' ? '' : state}`} role={label ? 'img' : undefined} aria-label={label} aria-hidden={label ? undefined : true} />
}

/** A hint, shown once, beside what it explains: its arrow points at it. */
export function Hint({ children, side = 'left', style, onDone }: {
  children: ReactNode
  side?: 'left' | 'right' | 'up' | 'down'
  style?: React.CSSProperties
  onDone?: () => void
}) {
  return (
    <div className={`tip ${side}`} role="note" style={style}>
      <div>{children}</div>
      <p className="shown">
        Shown once.{onDone && <> <button type="button" onClick={onDone}>Got it</button></>}
      </p>
    </div>
  )
}
