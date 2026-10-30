// Signing in to the shared graph: with GitHub, or with Google. Either only
// tells the graph who you are; it keeps a number for your account.

import { Button } from '../ui/ui'

export type Provider = 'github' | 'google'

export function SignInButtons({ signIn, small }: { signIn: (provider: Provider) => void; small?: boolean }) {
  return (
    <>
      <p className="row">
        <Button small={small} onClick={() => signIn('github')}>Sign in with GitHub</Button>
        <Button small={small} onClick={() => signIn('google')}>Sign in with Google</Button>
      </p>
      <Agreeing />
    </>
  )
}

/** Under every way to sign in: signing in is agreeing to the terms (Nathan, 29 October). */
export function Agreeing() {
  return (
    <p className="d agreeing">
      Signing in, you agree to the shared graph’s <a className="l" href="#/adding">terms</a>.
    </p>
  )
}
