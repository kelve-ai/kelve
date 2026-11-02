// What this tab showed is open in a newer one, which kelve opened to show it
// again. A tab you've moved around in can't close itself, so it says so.

import { Loader } from '../Loader'
import { Quiet } from '../ui/ui'

export function Moved({ showHere }: { showHere?: () => void }) {
  return (
    <div className="page">
      <main className="col" style={{ marginTop: 160 }}>
        <Loader size="medium" still label="kelve" />
        <h1 className="h1" style={{ marginTop: 26 }}>This is open in a newer tab.</h1>
        <p className="p soft" style={{ marginTop: 14 }}>
          kelve showed it again there, up to date. You can close this tab.
        </p>
        <p className="p" style={{ marginTop: 14 }}>
          <Quiet onClick={showHere}>Show it here instead</Quiet>
        </p>
      </main>
    </div>
  )
}
