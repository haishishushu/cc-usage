import { execFileSync } from "node:child_process"
import { existsSync, mkdtempSync, readdirSync, rmSync, statSync } from "node:fs"
import { join, resolve, sep } from "node:path"
import { pathToFileURL } from "node:url"

const REPO = "haishishushu/cc-usage"
const SUFFIXES = [
  "Windows-x86_64-Setup.exe", "Windows-x86_64-Setup.exe.sig",
  "macOS-arm64.dmg", "macOS-x86_64.dmg",
  "Linux-x86_64.AppImage", "Linux-x86_64.AppImage.sig",
  "Linux-x86_64.deb", "Linux-x86_64.deb.sig",
]

// 从已成功的 GitHub Actions 运行及资源上传时间逐版核实的源码提交。
const COMMITS = {
  "0.1.9": "749f3dedb045838b4063b93b5f03ec38ec87507b",
  "0.1.10": "0e839980706fe607ffb291ea7ded2ce039adf0ac",
  "0.1.11": "adf6cebf4817c19fe1c604ba8ec3e09705908240",
  "0.1.12": "8837fe65c419de3e2ffb7304a0d5ec214a3b5031",
  "0.1.13": "d3e4af38dcda843053dcf7496c12ba30af6d92ec",
}

export function planLegacyReleases(assets) {
  const groups = new Map()
  for (const asset of assets) {
    if (asset.name === "latest.json") continue
    const match = /^CC-Usage-v(\d+\.\d+\.\d+)-(.+)$/.exec(asset.name)
    if (!match || !COMMITS[match[1]]) throw new Error(`未核实的旧版资源：${asset.name}`)
    if (!groups.has(match[1])) groups.set(match[1], [])
    groups.get(match[1]).push(asset)
  }
  return [...groups].sort(([a], [b]) => {
    const left = a.split(".").map(Number)
    const right = b.split(".").map(Number)
    for (let index = 0; index < 3; index += 1) {
      if (left[index] !== right[index]) return left[index] - right[index]
    }
    return 0
  }).map(([version, files]) => {
    const expected = SUFFIXES.map((suffix) => `CC-Usage-v${version}-${suffix}`)
    const actual = new Set(files.map((asset) => asset.name))
    for (const name of expected) if (!actual.has(name)) throw new Error(`${version} 缺少 ${name}`)
    if (actual.size !== expected.length || files.length !== expected.length) throw new Error(`${version} 包含重复或未知资源`)
    return { version, tag: `v${version}`, commit: COMMITS[version], assets: files }
  })
}

function gh(args) {
  return execFileSync("gh", [...args, "--repo", REPO], { encoding: "utf8", stdio: ["inherit", "pipe", "inherit"] }).trim()
}

function ghJson(args) { return JSON.parse(gh(args)) }

async function assertSafeToArchiveLegacy(plan) {
  const latest = ghJson(["release", "list", "--limit", "30", "--json", "tagName,isLatest"])
    .find((release) => release.isLatest)?.tagName
  if (!latest || latest === "continuous" || plan.some(({ tag }) => tag === latest)) {
    throw new Error("新版独立发行版尚未成为最新版本，不能归档 continuous")
  }
  const response = await fetch("https://haishishushu.github.io/cc-usage/latest.json", { cache: "no-store" })
  if (!response.ok) throw new Error(`无法核实 Pages 更新清单：HTTP ${response.status}`)
  const manifest = await response.json()
  const urls = Object.values(manifest.platforms ?? {}).map((platform) => platform.url)
  if (manifest.version !== latest.slice(1) || urls.length === 0 ||
      urls.some((url) => typeof url !== "string" || !url.includes(`/releases/download/${latest}/`))) {
    throw new Error("Pages 更新清单尚未指向最新的独立发行版，不能归档旧发行版")
  }
}

function matchingAssets(actual, expected) {
  const names = new Set(actual.map((asset) => asset.name))
  return names.size === expected.length && expected.every((asset) => names.has(asset.name))
}

function migrateOne(release, existingTags) {
  const { version, tag, commit, assets } = release
  const folder = mkdtempSync(join(process.cwd(), ".release-migration-"))
  const root = resolve(process.cwd()) + sep
  if (!resolve(folder).startsWith(root)) throw new Error("迁移临时目录不在当前仓库内")
  try {
    gh(["release", "download", "continuous", "--pattern", `CC-Usage-v${version}-*`, "--dir", folder])
    const downloaded = new Set(readdirSync(folder))
    if (downloaded.size !== assets.length) throw new Error(`${version} 下载的资源数量不对`)
    for (const asset of assets) {
      const path = join(folder, asset.name)
      if (!downloaded.has(asset.name) || !existsSync(path) || statSync(path).size !== asset.size) {
        throw new Error(`${version} 下载资源与原发行版不一致：${asset.name}`)
      }
    }

    if (!existingTags.has(tag)) {
      gh(["release", "create", tag, "--target", commit, "--title", `CC Usage v${version}`,
        "--notes", `历史持续构建按版本归档。此页仅包含 v${version} 的原始安装包；对应源码提交 ${commit}。`,
        "--draft", "--latest=false"])
      existingTags.add(tag)
    }
    const current = ghJson(["release", "view", tag, "--json", "isDraft,assets"])
    const unexpected = current.assets.filter((asset) => !assets.some((expected) => expected.name === asset.name))
    if (unexpected.length || (!current.isDraft && !matchingAssets(current.assets, assets))) {
      throw new Error(`${tag} 已存在不匹配的发行资源，停止以免覆盖`)
    }
    const present = new Set(current.assets.map((asset) => asset.name))
    const missing = assets.filter((asset) => !present.has(asset.name))
    if (missing.length) gh(["release", "upload", tag, ...missing.map((asset) => join(folder, asset.name))])
    const verified = ghJson(["release", "view", tag, "--json", "assets"])
    if (!matchingAssets(verified.assets, assets)) throw new Error(`${tag} 上传后资源仍不完整`)
    if (current.isDraft) gh(["release", "edit", tag, "--draft=false", "--latest=false"])
    console.log(`${tag} 已独立发布：${assets.length} 个对应版本资源`)
  } finally {
    rmSync(folder, { recursive: true, force: true })
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const apply = process.argv.includes("--apply")
  const legacy = ghJson(["release", "view", "continuous", "--json", "assets"])
  const plan = planLegacyReleases(legacy.assets)
  console.log(JSON.stringify(plan.map(({ version, tag, commit, assets }) => ({ version, tag, commit, assetCount: assets.length })), null, 2))
  if (!apply) {
    console.log("只读预览；添加 --apply 才会创建发行版并归档 continuous。")
  } else {
    await assertSafeToArchiveLegacy(plan)
    const existingTags = new Set(ghJson(["release", "list", "--limit", "100", "--json", "tagName"]).map((release) => release.tagName))
    for (const release of plan) migrateOne(release, existingTags)
    gh(["release", "edit", "continuous", "--draft"])
    console.log("旧 continuous 发行版已转为草稿；资源仍保留以备核查。")
  }
}
