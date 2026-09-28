import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { pathToFileURL } from "node:url"

const VERSION_PATTERN = /^(\d+)\.(\d+)\.(\d+)$/
const ASSET_PATTERN = /^CC-Usage-v(\d+\.\d+\.\d+)-/

function parseVersion(value) {
  const match = VERSION_PATTERN.exec(value)
  if (!match) throw new Error(`无效版本号：${value}`)
  const parts = match.slice(1).map(Number)
  if (parts.some((part) => !Number.isSafeInteger(part))) throw new Error(`版本号超出安全整数范围：${value}`)
  return parts
}

function isNewer(left, right) {
  for (let index = 0; index < left.length; index += 1) {
    if (left[index] !== right[index]) return left[index] > right[index]
  }
  return false
}

export function nextReleaseVersion(configVersion, assets = [], tags = []) {
  let latest = parseVersion(configVersion)
  for (const asset of assets) {
    const match = ASSET_PATTERN.exec(asset.name ?? "")
    if (!match) continue
    const candidate = parseVersion(match[1])
    if (isNewer(candidate, latest)) latest = candidate
  }
  for (const tag of tags) {
    const match = /^v(\d+\.\d+\.\d+)$/.exec(tag.trim())
    if (!match) continue
    const candidate = parseVersion(match[1])
    if (isNewer(candidate, latest)) latest = candidate
  }

  const [major, minor, patch] = latest
  return patch >= 100 ? `${major}.${minor + 1}.0` : `${major}.${minor}.${patch + 1}`
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const release = JSON.parse(readFileSync(process.argv[2], "utf8"))
  const config = JSON.parse(readFileSync("backend/tauri.conf.json", "utf8"))
  const tags = process.argv[3] ? readFileSync(process.argv[3], "utf8").split(/\r?\n/) : []
  console.log(nextReleaseVersion(config.version, release.assets ?? [], tags))
}
