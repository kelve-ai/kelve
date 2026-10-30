// This browser isn't signed in. There's nothing to sign in with here: kelve
// opens itself with a one-time link.

import { Loader } from '../Loader'

export function SignedOut() {
  return (
    <div className="page">
      <main className="col" style={{ marginTop: 160 }}>
        <Loader size="medium" still label="kelve, waiting" />
        <h1 className="h1" style={{ marginTop: 26 }}>This browser isn’t signed in to kelve.</h1>
        <p className="p soft" style={{ marginTop: 14 }}>
          Open kelve with <span className="mono">kelve open</span>.
        </p>
      </main>
    </div>
  )
}
