import { createRoot } from 'react-dom/client'
import { App } from './App'
import { watchPage } from './api/client'
import './styles/app.css'
import './styles/figures.css'
import './styles/reader.css'
import './styles/ui.css'
import './styles/screens.css'
import './styles/list.css'

// Every screen opens at its top, not where the browser left it.
history.scrollRestoration = 'manual'
// How code was drawn last time, so it's drawn so from the first paint,
// before settings arrive.
try {
  const drawn = JSON.parse(localStorage.getItem('kelve.drawn') ?? 'null') as Record<string, string> | null
  if (drawn) Object.assign(document.documentElement.dataset, drawn)
} catch {
  // No storage, or nothing kept: settings draw it when they come.
}
watchPage()

createRoot(document.getElementById('root')!).render(<App />)
