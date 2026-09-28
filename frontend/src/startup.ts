import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"

const title = document.getElementById("title")!
const detail = document.getElementById("detail")!
const actions = document.getElementById("actions")!
let slowTimer: number | undefined
let errorTimer: number | undefined

const showError = (message: string) => {
  window.clearTimeout(slowTimer)
  window.clearTimeout(errorTimer)
  document.body.classList.add("failed")
  title.textContent = "主面板未能打开"
  detail.textContent = message || "请重试；若问题持续，请检查本地数据目录。"
  actions.hidden = false
}

void listen<string>("startup-error", (event) => {
  showError(event.payload)
})
void invoke<string | null>("startup_error").then((error) => { if (error) showError(error) }).catch(() => {})

document.getElementById("retry")!.addEventListener("click", () => {
  document.body.classList.remove("failed")
  title.textContent = "正在重新打开…"
  detail.textContent = "正在准备本地数据"
  actions.hidden = true
  scheduleWaitingMessages()
  void invoke<string>("retry_startup")
    .then((status) => {
      if (status === "initializing") detail.textContent = "本地数据仍在准备，请稍候…"
    })
    .catch((error) => showError(String(error || "重试失败，请退出后重新打开。")))
})

document.getElementById("quit")!.addEventListener("click", () => { void invoke("quit_startup") })

function scheduleWaitingMessages() {
  window.clearTimeout(slowTimer)
  window.clearTimeout(errorTimer)
  slowTimer = window.setTimeout(() => {
    if (actions.hidden) detail.textContent = "启动时间较长，正在继续准备…"
  }, 7000)
  errorTimer = window.setTimeout(() => {
    if (actions.hidden) showError("启动时间较长，请点击重试。")
  }, 15000)
}

scheduleWaitingMessages()
