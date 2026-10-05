import { spawnSync } from 'node:child_process'
import { cpSync, existsSync, mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = dirname(dirname(fileURLToPath(import.meta.url)))
const dist = join(root, 'docs/.vitepress/dist')

function cargo(args, capture = false) {
  const result = spawnSync('cargo', args, {
    cwd: root,
    encoding: 'utf8',
    stdio: capture ? ['ignore', 'pipe', 'inherit'] : 'inherit',
  })
  if (result.error) throw new Error(`Cargo is required to build the API reference: ${result.error.message}`)
  if (result.status !== 0) process.exit(result.status ?? 1)
  return result.stdout
}

// No publish command, credentials, deployment, or external host is involved.
cargo(['doc', '--workspace', '--no-deps', '--locked'])
const metadata = JSON.parse(cargo(['metadata', '--format-version', '1', '--no-deps', '--locked'], true))
const api = join(dist, 'api')
mkdirSync(api, { recursive: true })
cpSync(join(metadata.target_directory, 'doc'), api, { recursive: true })

for (const name of ['scriptaro_core', 'scriptaro_platform', 'scriptaro_engine', 'scriptaro_platform_macos', 'scriptaro_desktop']) {
  if (!existsSync(join(api, name, 'index.html'))) throw new Error(`Missing API reference: ${name}`)
}
writeFileSync(join(dist, '.nojekyll'), '')
console.log('Documentation and Rust API reference built locally in docs/.vitepress/dist')
