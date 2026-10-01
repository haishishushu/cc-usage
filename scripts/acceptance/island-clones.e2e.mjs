// 灵动岛分身端到端验收（画布 22）：通过 WebView2 远程调试口驱动隔离运行的开发版应用。
// 前置：按 README「隔离桌面流程」启动应用（CC_USAGE_TEST_DIR + --remote-debugging-port=9337），
//       并在能解析 `playwright` 包的目录下运行（例如 output/acceptance-clones 里 npm install playwright）。
// 用法：CC_USAGE_TEST_DIR=<隔离目录> node scripts/acceptance/island-clones.e2e.mjs [main|after-restart]
//   main：仅本体 → 开启两个分身 → 核对总数 → 销毁本体由分身接任，最后留一个分身。
//   after-restart：结束并重启应用后运行，核对分身窗口按设置重建，再销毁回到只剩本体。
import { chromium } from "playwright"
import { readFileSync } from "node:fs"
import { resolve } from "node:path"

const CDP = "http://127.0.0.1:9337"
const SETTINGS = resolve(process.env.CC_USAGE_TEST_DIR ?? "data", "settings.json")
const phase = process.argv[2] ?? "main"
const log = (...args) => console.log(new Date().toISOString().slice(11, 19), ...args)
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
const settings = () => JSON.parse(readFileSync(SETTINGS, "utf8"))

async function waitFor(label, fn, timeout = 20000) {
  const started = Date.now()
  for (;;) {
    const value = await fn()
    if (value) return value
    if (Date.now() - started > timeout) throw new Error("超时：" + label)
    await sleep(200)
  }
}

async function connect() {
  return waitFor("连接 CDP", async () => {
    try { return await chromium.connectOverCDP(CDP, { timeout: 3000 }) } catch { return null }
  }, 120000)
}

const browser = await connect()
const ctx = browser.contexts()[0]
const pages = () => ctx.pages().filter((p) => !p.isClosed())
const urls = () => pages().map((p) => p.url())
const islandPages = () => pages().filter((p) => p.url().includes("window=island"))
const cloneIdOf = (page) => new URL(page.url()).searchParams.get("clone")
const primaryPage = () => islandPages().find((p) => !cloneIdOf(p))
const clonePages = () => islandPages().filter((p) => cloneIdOf(p))
const menuPage = () => pages().find((p) => p.url().includes("window=menu"))

const watched = new WeakSet()
function watchConsole(page, name) {
  if (watched.has(page)) return
  watched.add(page)
  page.on("console", (m) => { if (m.type() === "error" || m.type() === "warning") log(name + " 控制台", m.type(), m.text().slice(0, 200)) })
  page.on("pageerror", (e) => log(name + " 页面错误", e.message))
}

async function openMenu(island) {
  watchConsole(island, "岛")
  const rect = await island.evaluate(() => { const r = document.body.getBoundingClientRect(); return { w: r.width, h: r.height } })
  const x = Math.max(8, Math.min(rect.w - 8, rect.w / 2))
  const y = Math.max(8, Math.min(rect.h - 8, rect.h / 2))
  // 真实右键：经 CDP 输入事件产生 contextmenu，前端调用 island_menu 命令。
  await island.mouse.move(x, y)
  await island.mouse.click(x, y, { button: "right" })
  log("右键", JSON.stringify({ x, y, rect }))
  const menu = await waitFor("菜单窗口出现", async () => menuPage())
  watchConsole(menu, "菜单")
  await menu.getByRole("menuitem", { name: "打开主面板", exact: true }).waitFor({ timeout: 10000 })
  await sleep(300)
  return menu
}

async function menuState(menu) {
  const count = await menu.locator('[aria-label^="当前 "]').textContent()
  const create = await menu.getByRole("menuitem", { name: "开启分身", exact: true }).isDisabled()
  const destroy = await menu.getByRole("menuitem", { name: "销毁分身", exact: true }).isDisabled()
  const labels = await menu.getByRole("menu", { name: "灵动岛菜单", exact: true }).locator("button").allTextContents()
  return { count, createDisabled: create, destroyDisabled: destroy, labels }
}

function assert(cond, message) { if (!cond) throw new Error("断言失败：" + message) }

const results = {}
try {
  await waitFor("本体灵动岛页面", async () => primaryPage(), 120000)
  await primaryPage().waitForSelector("#root > *", { timeout: 60000 })
  log("页面：", urls())

  if (phase === "after-restart") {
    await waitFor("重启后分身窗口重建", async () => clonePages().length >= 1, 60000)
    const saved = settings()
    results.restart = { clonePages: clonePages().map(cloneIdOf), saved: saved.island_clones.map((c) => ({ id: c.id, position: c.position, dock: c.dock.edge })) }
    assert(clonePages().length === saved.island_clones.length, "重启后窗口数与设置不一致")
    assert(clonePages().every((p) => saved.island_clones.some((c) => c.id === cloneIdOf(p))), "重启后分身 id 不匹配")
    // 右键分身检查总数后，再销毁它，恢复到只剩本体。
    const menu = await openMenu(clonePages()[0])
    const state = await menuState(menu)
    assert(state.count === String(1 + saved.island_clones.length), "重启后总数错误：" + state.count)
    await menu.getByRole("menuitem", { name: "销毁分身", exact: true }).click()
    await waitFor("分身窗口关闭", async () => clonePages().length === 0)
    assert(settings().island_clones.length === 0, "销毁后设置未清空")
    results.restart.destroyedAfterRestart = true
    log("重启验证通过", JSON.stringify(results.restart))
    console.log("RESULT " + JSON.stringify(results))
    process.exit(0)
  }

  // 1. 仅本体：总数 1，销毁禁用
  let menu = await openMenu(primaryPage())
  let state = await menuState(menu)
  results.onlyPrimary = state
  assert(state.count === "1", "仅本体时总数应为 1，实际 " + state.count)
  assert(state.destroyDisabled && !state.createDisabled, "仅本体时销毁应禁用、开启应可用")
  assert(state.labels.join("|") === "打开主面板|立即刷新|切换连接|显示位置|始终置顶|开启分身|销毁分身", "菜单顺序：" + state.labels.join("|"))

  // 2. 开启分身：新窗口出现，设置落盘
  await menu.getByRole("menuitem", { name: "开启分身", exact: true }).click()
  await waitFor("分身窗口出现", async () => clonePages().length === 1)
  const cloneA = clonePages()[0]
  await cloneA.waitForSelector("#root > *", { timeout: 30000 })
  await waitFor("分身写入设置", async () => settings().island_clones.length === 1)
  const savedA = settings().island_clones[0]
  results.created = { id: cloneIdOf(cloneA), saved: savedA.id, position: savedA.position, connection: savedA.connection_id }
  assert(savedA.id === cloneIdOf(cloneA), "分身窗口 id 与设置不一致")
  assert(Array.isArray(savedA.position), "分身自由态位置未记录")

  // 3. 分身的菜单：总数 2，销毁可用；再开一个 → 3
  menu = await openMenu(cloneA)
  state = await menuState(menu)
  results.cloneMenu = state
  assert(state.count === "2" && !state.destroyDisabled, "分身菜单应显示 2 且销毁可用")
  await menu.getByRole("menuitem", { name: "开启分身", exact: true }).click()
  await waitFor("第二个分身出现", async () => clonePages().length === 2)
  await waitFor("第二个分身写入设置", async () => settings().island_clones.length === 2)
  menu = await openMenu(primaryPage())
  state = await menuState(menu)
  results.threeIslands = state
  assert(state.count === "3", "三个岛时总数应为 3，实际 " + state.count)

  // 4. 销毁本体：第一个分身接任，本体窗口仍在，总数回到 2
  const firstCloneId = settings().island_clones[0].id
  await menu.getByRole("menuitem", { name: "销毁分身", exact: true }).click()
  await waitFor("接任的分身窗口关闭", async () => !clonePages().some((p) => cloneIdOf(p) === firstCloneId))
  await waitFor("设置更新", async () => settings().island_clones.length === 1)
  assert(primaryPage() && !primaryPage().isClosed(), "本体窗口不应消失")
  menu = await openMenu(primaryPage())
  state = await menuState(menu)
  results.afterPromote = { ...state, remaining: settings().island_clones.map((c) => c.id) }
  assert(state.count === "2", "接任后总数应为 2，实际 " + state.count)
  await menu.press("body", "Escape").catch(() => {})
  await sleep(300)

  // 5. 留一个分身用于重启验证
  results.leftForRestart = settings().island_clones.map((c) => c.id)
  log("主流程通过", JSON.stringify(results))
  console.log("RESULT " + JSON.stringify(results))
  process.exit(0)
} catch (error) {
  console.error("FAILED", error.message)
  console.error("pages:", urls())
  try { console.error("settings:", JSON.stringify(settings().island_clones)) } catch {}
  process.exit(1)
}
