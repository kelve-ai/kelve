// kelve's controls, as the canvas draws them (styles in kit.css): buttons, a
// segmented choice, a switch, a field, something to copy, a dialog over the
// page, a toast, a spinner, a key, a chip and a red text link. One main
// action a screen, and it names what it does.

import { type ReactNode, useCallback, useEffect, useId, useRef, useState } from 'react'

type BtnProps = {
  children?: ReactNode
  /** `primary`: the one main action; `secondary`: a white key beside it; `quiet`: words until pointed at. */
  kind?: 'primary' | 'secondary' | 'quiet'
  size?: 'small' | 'large'
  /** Only an icon: `label` says what it does. */
  icon?: boolean
  label?: string
  /** A toggle: on or off, read aloud as pressed. */
  pressed?: boolean
  /** A link that looks like a button. */
  href?: string
  onClick?: () => void
  disabled?: boolean
  type?: 'button' | 'submit'
  title?: string
}

const KIND = { primary: 'btn-p', secondary: 'btn-s', quiet: 'btn-g' }

export function Btn({ children, kind = 'secondary', size, icon, label, pressed, href, onClick, disabled, type = 'button', title }: BtnProps) {
  const cls = ['btn', KIND[kind], size === 'small' && 'btn-sm', size === 'large' && 'btn-lg', icon && 'btn-icon'].filter(Boolean).join(' ')
  if (href) {
    return (
      <a className={cls} href={disabled ? undefined : href} aria-disabled={disabled || undefined} aria-label={label} title={title ?? (icon ? label : undefined)} onClick={onClick}>
        {children}
      </a>
    )
  }
  return (
    <button type={type} className={cls} onClick={onClick} disabled={disabled} aria-label={label} aria-pressed={pressed} title={title ?? (icon ? label : undefined)}>
      {children}
    </button>
  )
}

/** A red text link: to another page with `href`, or an action with `onClick`. */
export function TextLink({ children, href, onClick }: { children: ReactNode; href?: string; onClick?: () => void }) {
  if (href) return <a className="go" href={href} onClick={onClick}>{children}</a>
  return <button type="button" className="go" onClick={onClick}>{children}</button>
}

/** A choice of a few, side by side: the chosen one a white key on a soft track. */
export function Segmented<T extends string>({ options, value, onChange, label, small }: {
  options: Array<{ value: T; label: ReactNode }>
  value: T
  onChange: (value: T) => void
  label: string
  small?: boolean
}) {
  return (
    <div className={`seg${small ? ' sm' : ''}`} role="group" aria-label={label}>
      {options.map((o) => (
        <button key={o.value} type="button" aria-pressed={o.value === value} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  )
}

/** On or off, red when on. `label` is what it turns on. */
export function Toggle({ on, onChange, label, disabled }: { on: boolean; onChange: (on: boolean) => void; label: string; disabled?: boolean }) {
  return <button type="button" className="sw" role="switch" aria-checked={on} aria-label={label} disabled={disabled} onClick={() => onChange(!on)} />
}

/** A text field with its label above it, and a line under it when it needs one. */
export function TextField({ label, value, onChange, placeholder, hint, multiline, maxLength, hideLabel, autoFocus }: {
  label: string
  value: string
  onChange: (value: string) => void
  placeholder?: string
  hint?: ReactNode
  multiline?: boolean
  maxLength?: number
  /** Labelled for screen readers only, where the page says it another way. */
  hideLabel?: boolean
  autoFocus?: boolean
}) {
  const id = useId()
  const props = { id, className: 'field', value, placeholder, maxLength, autoFocus, 'aria-describedby': hint ? `${id}-hint` : undefined }
  return (
    <div className="field-at">
      <label htmlFor={id} className={hideLabel ? 'sr-only' : undefined}>{label}</label>
      {multiline ? <textarea {...props} rows={4} onChange={(e) => onChange(e.target.value)} /> : <input {...props} type="text" onChange={(e) => onChange(e.target.value)} />}
      {hint && <small id={`${id}-hint`}>{hint}</small>}
    </div>
  )
}

/**
 * Something to copy: a command, with a `$` before it, or words to say to an
 * agent, without one. Copy says Copied for a moment.
 */
export function CopyLine({ text, words }: { text: string; words?: boolean }) {
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
    <div className={`copy${words ? ' words' : ''}`}>
      <span className="cwell">{!words && <span className="ps" aria-hidden="true">$ </span>}{text}</span>
      <Btn size="small" onClick={copy} label={copied ? 'Copied' : `Copy ${words ? 'the words' : 'the command'}`}>{copied ? 'Copied' : 'Copy'}</Btn>
    </div>
  )
}

/**
 * A dialog over the page: a sheet on a soft scrim. Escape or a click on the
 * scrim closes it; focus goes into it, and back where it was after.
 */
export function Dialog({ title, children, actions, onClose }: {
  title: string
  children?: ReactNode
  /** Its buttons, the main one last. */
  actions?: ReactNode
  onClose: () => void
}) {
  const id = useId()
  const sheet = useRef<HTMLDivElement>(null)
  // The latest onClose, so focus moves in once, not each time the page draws.
  const close = useRef(onClose)
  useEffect(() => {
    close.current = onClose
  })
  useEffect(() => {
    const before = document.activeElement as HTMLElement | null
    const first = sheet.current?.querySelector<HTMLElement>('input, textarea, button, a[href]')
    ;(first ?? sheet.current)?.focus()
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close.current()
    }
    window.addEventListener('keydown', key)
    return () => {
      window.removeEventListener('keydown', key)
      before?.focus()
    }
  }, [])
  return (
    <div className="scrim" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="sheet" role="dialog" aria-modal="true" aria-labelledby={id} ref={sheet} tabIndex={-1}>
        <h2 id={id}>{title}</h2>
        {children}
        {actions && <div className="acts">{actions}</div>}
      </div>
    </div>
  )
}

/** A line at the foot of the page, read aloud as it comes, with one thing to do at most. */
export function Toast({ children, action }: { children: ReactNode; action?: { label: string; run: () => void } }) {
  return (
    <div className="toast" role="status">
      <span>{children}</span>
      {action && <button type="button" onClick={action.run}>{action.label}</button>}
    </div>
  )
}

/** A toast that goes by itself: `say` shows a line for a few seconds. */
export function useToast(ms = 2600): [ReactNode, (text: string) => void] {
  const [text, setText] = useState('')
  useEffect(() => {
    if (!text) return
    const t = window.setTimeout(() => setText(''), ms)
    return () => window.clearTimeout(t)
  }, [text, ms])
  const say = useCallback((t: string) => setText(t), [])
  return [text ? <Toast>{text}</Toast> : null, say]
}

/** Something on its way: a small red ring, with words saying what's happening. */
export function Spinner({ children, label = 'Working' }: { children?: ReactNode; label?: string }) {
  if (!children) return <span className="spin" role="status" aria-label={label} />
  return (
    <span className="busy" role="status">
      <span className="spin" aria-hidden="true" />
      {children}
    </span>
  )
}

/** A key on the keyboard. */
export function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="kbd">{children}</kbd>
}

/** A kind of event, in its colour: a debrief, a check, a skill, a score, a new part, or done. */
export type Tone = 'debrief' | 'check' | 'skill' | 'score' | 'part' | 'done'

/** A few words on a soft pill, in a kind's colour. */
export function Chip({ children, tone }: { children: ReactNode; tone?: Tone }) {
  return <span className={`chip${tone ? ` ${tone}` : ''}`}>{children}</span>
}
