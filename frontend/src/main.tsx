import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import App from "./App"
import "./index.css"

// 在首帧之前标记灵动岛窗口，避免白色背景闪一下
if (["island", "menu", "tray-summary"].includes(new URLSearchParams(window.location.search).get("window") ?? "")) {
  document.documentElement.classList.add("island-window")
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
