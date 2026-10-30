// Diagrams for the states gallery, so how they're drawn is guarded: the
// example's entries are written in paragraphs, and an agent's diagrams can
// hold many nodes and long labels, unbroken ones too. `as=diagrams` gives
// two of the example's entries these.

import type { Block } from '../api/types'

/** Unique keys: emails kept unique ignoring case, seven nodes in a row, some labels one long word. */
export const UNIQUE_KEYS: Block[] = [
  {
    id: 'd1',
    part: 'how',
    body: {
      type: 'diagram',
      layout: 'row',
      nodes: [
        { id: 'a', label: 'Ann@Example.com', note: null, mark: false },
        { id: 'b', label: 'ann@example.com', note: null, mark: false },
        { id: 'c', label: 'ANN@EXAMPLE.COM', note: null, mark: false },
        { id: 'd', label: 'Bob@example.com', note: null, mark: false },
        { id: 'e', label: 'bob@example.com', note: null, mark: false },
        { id: 'A', label: 'class: ann@example.com', note: 'three spellings', mark: true },
        { id: 'B', label: 'class: bob@example.com', note: 'two spellings', mark: false },
      ],
      edges: [
        { from: 'a', to: 'A', label: null },
        { from: 'b', to: 'A', label: null },
        { from: 'c', to: 'A', label: null },
        { from: 'd', to: 'B', label: null },
        { from: 'e', to: 'B', label: null },
      ],
      steps: [],
      describe: 'Five spellings of email addresses, compared ignoring case, fall into two classes: three spellings of Ann’s address in one, two of Bob’s in the other. No spelling is in both.',
    },
  },
  { id: 'd2', part: 'how', body: { type: 'paragraph', text: 'A unique key that ignores case keeps one row for each class, whichever spelling came first.', quiet: false } },
]

/** Exponential backoff: each try and its wait, on a timeline, with long labels and notes. */
export const BACKOFF: Block[] = [
  {
    id: 'd1',
    part: 'how',
    body: {
      type: 'diagram',
      layout: 'timeline',
      nodes: [
        { id: 't1', label: 'First try, as the shop asks', note: 'the bank doesn’t answer within its 15-second limit', mark: false },
        { id: 't2', label: 'Second try, after about half a second', note: 'a wait picked at random between nothing and half a second', mark: false },
        { id: 't3', label: 'Third try, after about a second', note: 'twice the wait before, still at random', mark: false },
        { id: 't4', label: 'Fourth try, after about two seconds', note: 'the same Idempotency-Key: 9b2e4c1a-55d0-4f7e-8a31-c0ffee000001', mark: true },
        { id: 't5', label: 'Fifth try, the last', note: 'then the charge fails, and the shop is told', mark: false },
      ],
      edges: [
        { from: 't1', to: 't2', label: 'timed out after fifteen seconds' },
        { from: 't2', to: 't3', label: 'the bank was busy' },
        { from: 't3', to: 't4', label: null },
        { from: 't4', to: 't5', label: 'still busy, so it waits the longest of all' },
      ],
      steps: [],
      describe: 'A charge tried up to six times: each wait about twice the one before, picked at random, and every try carries the same idempotency key, so the bank charges once.',
    },
  },
  {
    id: 'd2',
    part: 'how',
    body: {
      type: 'diagram',
      layout: 'row',
      nodes: [
        { id: 'r', label: 'Retry loop', note: 'src/retry.rs', mark: false },
        { id: 'w', label: 'Wait = base × 2^tries, then a random part of it', note: 'full jitter, so callers spread out', mark: true },
        { id: 'b', label: 'Bank', note: null, mark: false },
      ],
      edges: [
        { from: 'r', to: 'w', label: 'after a failure that passes' },
        { from: 'w', to: 'b', label: 'tries again' },
      ],
      steps: [],
      describe: 'The retry loop waits a random part of a wait that doubles with each try, then calls the bank again.',
    },
  },
  { id: 'd3', part: 'how', body: { type: 'paragraph', text: 'The waits double, so a few tries cover a long outage without calling the bank often.', quiet: false } },
]
