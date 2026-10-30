// The formulas an explorable can use (core's `Formula`). kelve computes
// them; the model only picks one and sets its inputs. Each result is in the
// units a guess's ranges use: money for a loan or growth, a percentage for a
// chance or a change, a plain number for agreement beyond chance (kappa).

import type { ExplorableInput, Formula } from '../api/types'

type Values = Record<string, number>

const at = (v: Values, name: string) => v[name] ?? 0

/** The chance, as a percentage, that at least k of n go one way, each with chance p. */
function atLeast(n: number, k: number, p: number): number {
  n = Math.max(0, Math.round(n))
  k = Math.max(0, Math.round(k))
  if (k > n) return 0
  let c = 1
  let sum = 0
  for (let i = 0; i <= n; i++) {
    if (i > 0) c = (c * (n - i + 1)) / i
    if (i >= k) sum += c * p ** i * (1 - p) ** (n - i)
  }
  return Math.min(100, Math.max(0, sum * 100))
}

export function compute(formula: Formula, v: Values): number {
  switch (formula) {
    case 'loanPayment': {
      // A month's payment on `principal`, at `rate` % a year, over `years`.
      const r = at(v, 'rate') / 100 / 12
      const n = at(v, 'years') * 12
      const p = at(v, 'principal')
      if (n <= 0) return 0
      return r === 0 ? p / n : (p * r) / (1 - (1 + r) ** -n)
    }
    case 'compoundGrowth':
      return at(v, 'start') * (1 + at(v, 'rate') / 100) ** at(v, 'years')
    case 'chanceAtLeast':
      return atLeast(at(v, 'n'), at(v, 'k'), at(v, 'p') / 100)
    case 'agreementBeyondChance': {
      const o = at(v, 'observed') / 100
      const c = at(v, 'byChance') / 100
      return c >= 1 ? 0 : (o - c) / (1 - c)
    }
    case 'percentChange': {
      const before = at(v, 'before')
      return before === 0 ? 0 : ((at(v, 'after') - before) / Math.abs(before)) * 100
    }
    case 'rateTimesTime':
      return at(v, 'rate') * at(v, 'time')
  }
}

/** A unit that goes before the number, like a currency's sign. */
const before = (unit: string | null) => !!unit && /^[^\p{L}\p{N}%\s]{1,3}$/u.test(unit)

function decimalsOf(step: number) {
  const s = String(step)
  return s.includes('.') ? Math.min(4, s.split('.')[1].length) : 0
}

const num = (v: number, decimals: number) =>
  v.toLocaleString('en-GB', { minimumFractionDigits: decimals, maximumFractionDigits: decimals })

const SIGNS: Record<string, string> = { dollars: '$', dollar: '$', pounds: '£', pound: '£', euros: '€', euro: '€' }

/** A unit that names money in words, as its sign and the rest: "dollars an hour" is "$" and "an hour". */
function money(unit: string | null): [string, string] | null {
  const m = unit?.match(/^(dollars?|pounds?|euros?)\b\s*(.*)$/i)
  return m ? [SIGNS[m[1].toLowerCase()], m[2]] : null
}

/** A number with its unit, said as a person would: "£250,000", "$0.09 an hour", "4.5%", "1 hour", "25 years". */
function withUnit(n: string, value: number, u: string | null): string {
  if (!u) return n
  if (before(u)) return `${u}${n}`
  if (u === '%') return `${n}%`
  const m = money(u)
  if (m) return m[1] ? `${m[0]}${n} ${m[1]}` : `${m[0]}${n}`
  return `${n} ${value === 1 && /^[a-z]+s$/i.test(u) ? u.slice(0, -1) : u}`
}

/** An input's value with its unit: "£250,000", "4.5%", "25 years". */
export function showInput(input: ExplorableInput, value: number): string {
  return withUnit(num(value, decimalsOf(input.step)), value, input.unit)
}

/** A result as it's read: money with the inputs' currency, a percentage, or kappa to two places. */
export function showResult(formula: Formula, value: number, inputs: ExplorableInput[]): string {
  switch (formula) {
    case 'loanPayment':
    case 'compoundGrowth': {
      const unit = inputs.find((i) => i.name === (formula === 'loanPayment' ? 'principal' : 'start'))?.unit ?? null
      const n = num(value, Math.abs(value) >= 100 ? 0 : 2)
      return before(unit) ? `${unit}${n}` : unit ? `${n} ${unit}` : n
    }
    case 'chanceAtLeast':
      if (value > 0 && value < 0.1) return 'under 0.1%'
      return `${num(value, value < 1 ? 1 : 0)}%`
    case 'agreementBeyondChance':
      return num(value, 2)
    case 'percentChange': {
      const sign = value > 0 ? '+' : value < 0 ? '−' : ''
      return `${sign}${num(Math.abs(value), Math.abs(value) < 10 ? 1 : 0)}%`
    }
    case 'rateTimesTime': {
      // Money when the rate is money: $0.09 an hour for 24 hours is $2.19.
      const rate = inputs.find((i) => i.name === 'rate')?.unit ?? null
      const sign = before(rate) ? rate : money(rate)?.[0]
      const n = num(value, Number.isInteger(value) && !sign ? 0 : 2)
      return sign ? `${sign}${n}` : n
    }
  }
}
