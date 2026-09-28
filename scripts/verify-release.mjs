import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { pathToFileURL } from "node:url"

const INSTALLERS = [
  "Windows-x86_64-Setup.exe", "macOS-arm64.dmg", "macOS-x86_64.dmg",
  "Linux-x86_64.AppImage", "Linux-x86_64.deb",
]

/** 发布 latest.json 前确认发行版、安装包及更新地址都属于同一版本。 */
export function verifyRelease({ version, tag, manifest, assets }) {
  if (!/^\d+\.\d+\.\d+$/.test(version) || tag !== `v${version}`) {
    throw new Error(`发行标签与版本不一致：${tag} / ${version}`)
  }
  if (manifest?.version !== version) {
    throw new Error(`更新清单版本不一致：${manifest?.version} / ${version}`)
  }

  const prefix = `CC-Usage-v${version}-`
  const names = new Set(assets.map((asset) => asset.name))
  for (const name of names) {
    if (name !== "latest.json" && !name.startsWith(prefix)) {
      throw new Error(`发行版混入其他版本资源：${name}`)
    }
  }
  for (const suffix of INSTALLERS) {
    if (!names.has(`${prefix}${suffix}`)) throw new Error(`缺少安装包：${prefix}${suffix}`)
  }
  if (!names.has("latest.json")) throw new Error("缺少 latest.json")

  const platforms = Object.values(manifest.platforms ?? {})
  if (platforms.length === 0) throw new Error("更新清单缺少平台")
  for (const platform of platforms) {
    const url = platform?.url
    if (typeof url !== "string") throw new Error("更新清单缺少下载地址")
    const marker = `/releases/download/${tag}/`
    if (!url.includes(marker)) throw new Error(`更新清单下载地址指向了其他发行版：${url}`)
    const name = decodeURIComponent(url.slice(url.lastIndexOf("/") + 1))
    if (!name.startsWith(prefix) || !names.has(name)) {
      throw new Error(`更新清单下载地址对应的安装包不存在：${url}`)
    }
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const [, , version, tag, manifestPath, releasePath] = process.argv
  if (!version || !tag || !manifestPath || !releasePath) {
    throw new Error("用法：node scripts/verify-release.mjs <version> <tag> <latest.json> <release.json>")
  }
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"))
  const release = JSON.parse(readFileSync(releasePath, "utf8"))
  verifyRelease({ version, tag, manifest, assets: release.assets ?? [] })
  console.log(`已验证 ${tag}：安装包及更新清单均属于 ${version}`)
}
