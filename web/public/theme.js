// Your light or dark choice, before the page draws, so it never flashes the
// other one. A file of its own, since kelve's pages run no inline scripts.
try {
  const t = localStorage.getItem('kelve:theme')
  if (t === 'light' || t === 'dark') document.documentElement.dataset.theme = t
} catch {}
