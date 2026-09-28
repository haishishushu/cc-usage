import assert from "node:assert/strict"
import test from "node:test"
import { verifyRelease } from "./verify-release.mjs"

const version = "0.1.14"
const tag = `v${version}`
const prefix = `CC-Usage-v${version}-`
const assets = [
  `${prefix}Windows-x86_64-Setup.exe`, `${prefix}Windows-x86_64-Setup.exe.sig`,
  `${prefix}macOS-arm64.dmg`, `${prefix}macOS-x86_64.dmg`,
  `${prefix}Linux-x86_64.AppImage`, `${prefix}Linux-x86_64.AppImage.sig`,
  `${prefix}Linux-x86_64.deb`, `${prefix}Linux-x86_64.deb.sig`, "latest.json",
].map((name) => ({ name }))
const manifest = {
  version,
  platforms: {
    "windows-x86_64": { url: `https://github.com/haishishushu/cc-usage/releases/download/${tag}/${prefix}Windows-x86_64-Setup.exe` },
    "linux-x86_64": { url: `https://github.com/haishishushu/cc-usage/releases/download/${tag}/${prefix}Linux-x86_64.AppImage` },
  },
}

test("独立发行版只包含自身版本的安装包与更新清单", () => {
  assert.doesNotThrow(() => verifyRelease({ version, tag, manifest, assets }))
})

test("混入旧版本安装包时拒绝标为最新", () => {
  assert.throws(() => verifyRelease({ version, tag, manifest, assets: [...assets, { name: "CC-Usage-v0.1.13-Windows-x86_64-Setup.exe" }] }), /其他版本/)
})

test("清单仍指向 continuous 时拒绝标为最新", () => {
  const stale = structuredClone(manifest)
  stale.platforms["windows-x86_64"].url = stale.platforms["windows-x86_64"].url.replace(tag, "continuous")
  assert.throws(() => verifyRelease({ version, tag, manifest: stale, assets }), /下载地址/)
})

test("缺少系统安装包时拒绝发布清单", () => {
  assert.throws(() => verifyRelease({ version, tag, manifest, assets: assets.filter((asset) => !asset.name.endsWith("macOS-arm64.dmg")) }), /缺少/)
})
