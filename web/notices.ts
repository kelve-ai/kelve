// Writes THIRD_PARTY_NOTICES.md: the code other people wrote that
// kelve includes, each with its license, as those licenses ask. The web app
// carries a copy (vite.config.ts), so the notices go wherever kelve goes.
//
//   bun run notices
//
// Run it again after changing dependencies: Rust crates come from Cargo.lock,
// through every crate the kelve binary is built from, and the web app's
// from the packages its own dependencies pull in.

import { execFileSync } from 'node:child_process'
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'

const web = import.meta.dirname
const root = resolve(web, '..')

interface Package {
  name: string
  version: string
  license: string
  repository: string
  /** The license files it ships, by name. */
  texts: Array<{ file: string; text: string }>
}

/** The license files in a package's folder: LICENSE, LICENSE-MIT, COPYING, NOTICE and the like. */
function licenseFiles(dir: string): Array<{ file: string; text: string }> {
  return readdirSync(dir)
    .filter(f => /^(licen[cs]e|copying|copyright|notice|unlicense)/i.test(f) && statSync(join(dir, f)).isFile())
    .sort()
    .map(file => ({ file, text: readFileSync(join(dir, file), 'utf8').replace(/\r\n/g, '\n').trim() }))
}

const meta = JSON.parse(execFileSync('cargo', ['metadata', '--format-version', '1', '--locked'], { cwd: root, maxBuffer: 1 << 28 }).toString())

/** Every crate one of kelve's binaries is built from, by id, but not kelve's own. */
function builtFrom(name: string): Set<string> {
  const ours = new Set<string>(meta.workspace_members)
  const nodes = new Map<string, any>(meta.resolve.nodes.map((n: any) => [n.id, n]))
  const start = meta.packages.find((p: any) => p.name === name && ours.has(p.id))
  const seen = new Set<string>()
  const stack: string[] = [start.id]
  while (stack.length) {
    const id = stack.pop()!
    if (seen.has(id)) continue
    seen.add(id)
    // What it's built with and from, but not what only its tests use.
    for (const dep of nodes.get(id).deps) if (dep.dep_kinds.some((k: any) => k.kind !== 'dev')) stack.push(dep.pkg)
  }
  return new Set([...seen].filter(id => !ours.has(id)))
}

/** The crates, as packages. */
function crates(ids: Set<string>): Package[] {
  const byId = new Map<string, any>(meta.packages.map((p: any) => [p.id, p]))
  return [...ids]
    .map(id => byId.get(id))
    .map(p => ({
      name: p.name,
      version: p.version,
      license: p.license ?? (p.license_file ? `see ${p.license_file}` : 'unknown'),
      repository: p.repository ?? '',
      texts: licenseFiles(dirname(p.manifest_path)),
    }))
    .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version))
}

/** Where a package's folder is, from the one that depends on it. */
function found(name: string, from: string): string | null {
  for (let dir = from; dir.startsWith(web); dir = dirname(dir)) {
    const at = join(dir, 'node_modules', name)
    if (existsSync(join(at, 'package.json'))) return at
    if (dir === web) break
  }
  return null
}

/** Every package the web app's own dependencies pull in. */
function packages(): Package[] {
  const own = JSON.parse(readFileSync(join(web, 'package.json'), 'utf8'))
  const queue: Array<[string, string]> = Object.keys(own.dependencies).map(n => [n, web])
  const seen = new Map<string, Package>()
  while (queue.length) {
    const [name, from] = queue.shift()!
    const dir = found(name, from)
    if (!dir || seen.has(dir)) continue
    const p = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'))
    const repo = typeof p.repository === 'string' ? p.repository : (p.repository?.url ?? '')
    seen.set(dir, { name, version: p.version, license: p.license ?? 'unknown', repository: repo.replace(/^git\+/, '').replace(/\.git$/, ''), texts: licenseFiles(dir) })
    for (const dep of Object.keys(p.dependencies ?? {})) queue.push([dep, dir])
  }
  return [...seen.values()].sort((a, b) => a.name.localeCompare(b.name))
}

/**
 * The Apache License 2.0, which many packages ship a copy of, each laid out a
 * little differently. A file much longer than the license holds others too,
 * like a library that bundles C code under several, and is printed whole.
 */
const isApache = (text: string) =>
  /Apache License\s+Version 2\.0, January 2004/.test(text) && /TERMS AND CONDITIONS FOR USE, REPRODUCTION/.test(text) && text.length < 12_500

/** The copyright lines in a copy of the Apache License, but not its template's. */
const copyrights = (text: string) => [...text.matchAll(/^\s*Copyright\s+(?!\[yyyy\]|\{yyyy\})(.+)$/gm)].map(m => `Copyright ${m[1].trim()}`)

const fenced = (text: string) => `\`\`\`text\n${text.replace(/```/g, "'''")}\n\`\`\``

/** The packages, then each license text once, with the packages it covers. */
function section(list: Package[]): string {
  const lines = list.map(p => `- ${p.name} ${p.version}, ${p.license}${p.repository ? `: <${p.repository}>` : ''}`)
  const groups = new Map<string, string[]>()
  // The Apache License is printed once, with the copyright lines from each package's copy.
  const apache = { covers: [] as string[], notices: [] as string[], counts: new Map<string, number>() }
  for (const p of list) {
    for (const t of p.texts) {
      const named = `${p.name} ${p.version} (${t.file})`
      if (isApache(t.text)) {
        apache.covers.push(named)
        apache.notices.push(...copyrights(t.text).map(c => `${c} (${p.name})`))
        apache.counts.set(t.text, (apache.counts.get(t.text) ?? 0) + 1)
        continue
      }
      const covers = groups.get(t.text) ?? []
      covers.push(named)
      groups.set(t.text, covers)
    }
  }
  const bare = list.filter(p => !p.texts.length).map(p => `${p.name} ${p.version}`)
  const texts = [...groups.entries()].map(([text, covers]) => `#### ${covers.join(', ')}\n\n${fenced(text)}`)
  if (apache.covers.length) {
    const usual = [...apache.counts.entries()].sort((a, b) => b[1] - a[1])[0][0]
    const notices = apache.notices.length ? `\n\nTheir copies of it carry these notices:\n\n${[...new Set(apache.notices)].map(n => `- ${n}`).join('\n')}` : ''
    texts.unshift(`#### The Apache License 2.0, for ${apache.covers.join(', ')}${notices}\n\n${fenced(usual)}`)
  }
  const note = bare.length
    ? `\n\nThese ship no license file of their own, and are under the standard text of the license named above: ${bare.join(', ')}.`
    : ''
  return `${lines.join('\n')}${note}\n\n### Their license texts\n\n${texts.join('\n\n')}`
}

const head = `# Third-party notices

kelve includes code that other people wrote, under their own
licenses. This file lists each, with its license text, as those licenses
ask. kelve's own license is [LICENSE](LICENSE).

It's made by \`bun run notices\` in \`web/\`, from \`Cargo.lock\` and the web
app's dependencies: run it again after changing them.`

const kelve = builtFrom('kelve')
writeFileSync(
  join(root, 'THIRD_PARTY_NOTICES.md'),
  `${head}\n\n## In the web app\n\n${section(packages())}\n\n## In the kelve binary\n\n${section(crates(kelve))}\n`,
)
console.log('Wrote THIRD_PARTY_NOTICES.md')
