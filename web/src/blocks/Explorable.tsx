// An explorable: your own numbers, to move. kelve knows the formula; the
// model only picked it and set its inputs. Nothing is asked: the number
// shows, and the sliders move it.

import { useState, type CSSProperties } from 'react'
import type { Body } from '../api/types'
import { compute, showInput, showResult } from './formula'

type Explorable = Extract<Body, { type: 'explorable' }>

/** How far along a slider is, for its inked line (figures.css). */
export const along = (v: number, min: number, max: number): CSSProperties => ({
  ['--p' as string]: `${max > min ? ((v - min) / (max - min)) * 100 : 0}%`,
})

const startOf = (body: Explorable) => Object.fromEntries(body.inputs.map((i) => [i.name, i.value]))

export function Explorable({ id, body }: { id: string; body: Explorable }) {
  const [values, setValues] = useState<Record<string, number>>(() => startOf(body))
  const result = compute(body.formula, values)
  return (
    <div className="ex">
      {body.inputs.map((input) => {
        const v = values[input.name] ?? input.value
        return (
          <div key={input.name} className="ex-in">
            <label htmlFor={`${id}-${input.name}`} className="ex-label">
              <span>{input.label}</span>
              <b>{showInput(input, v)}</b>
            </label>
            <input id={`${id}-${input.name}`} className="range" type="range" min={input.min} max={input.max} step={input.step} value={v}
              style={along(v, input.min, input.max)} onChange={(e) => setValues((s) => ({ ...s, [input.name]: Number(e.target.value) }))} />
            <div className="ex-ends" aria-hidden="true">
              <span>{showInput(input, input.min)}</span>
              <span>{showInput(input, input.max)}</span>
            </div>
          </div>
        )
      })}
      <div className="ex-out">
        <span>{body.output}</span>
        <b aria-live="polite">{showResult(body.formula, result, body.inputs)}</b>
      </div>
      <p className="describe">{body.describe}</p>
    </div>
  )
}
