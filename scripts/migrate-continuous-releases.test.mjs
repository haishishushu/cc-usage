import assert from "node:assert/strict"
import test from "node:test"
import { planLegacyReleases } from "./migrate-continuous-releases.mjs"

const suffixes = [
  "Windows-x86_64-Setup.exe", "Windows-x86_64-Setup.exe.sig",
  "macOS-arm64.dmg", "macOS-x86_64.dmg",
  "Linux-x86_64.AppImage", "Linux-x86_64.AppImage.sig",
  "Linux-x86_64.deb", "Linux-x86_64.deb.sig",
]
const assets = suffixes.map((suffix) => ({ name: `CC-Usage-v0.1.13-${suffix}`, size: 10 }))

test("迁移计划按版本分组并保留对应提交", () => {
  const plan = planLegacyReleases(assets)
  assert.equal(plan.length, 1)
  assert.equal(plan[0].tag, "v0.1.13")
  assert.equal(plan[0].assets.length, 8)
  assert.equal(plan[0].commit, "d3e4af38dcda843053dcf7496c12ba30af6d92ec")
})

test("缺包或未知版本时停止迁移，避免建错发行页", () => {
  assert.throws(() => planLegacyReleases(assets.slice(1)), /缺少/)
  assert.throws(() => planLegacyReleases([{ name: "CC-Usage-v0.1.8-Windows-x86_64-Setup.exe", size: 10 }]), /未核实/)
})
