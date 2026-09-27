import type { QuotaStateDto } from "./api.ts"
import type { PlatformId } from "../types.ts"

export interface RemainingRow {
  label: string
  value: string
  usedText: string | null
  resetsAt: string | null
}

export interface RemainingDisplay {
  title: string
  source: string
  rows: RemainingRow[]
  state: "ok" | "loading" | "unsupported" | "forbidden" | "unauthorized" | "rate_limited" | "failed"
  reason: string
}

const UNAVAILABLE: Partial<Record<PlatformId, { title: string; source: string; reason: string }>> = {
  gemini: {
    title: "Gemini 剩余额度",
    source: "Gemini API 项目 / 账号",
    reason: "本机 Gemini CLI 记录只包含消耗；当前未接入可验证的在线剩余额度查询。",
  },
  trae: {
    title: "Trae 剩余额度",
    source: "Trae 个人 / 企业账号",
    reason: "企业额度查询需要管理员授权；个人账号暂无可验证的剩余额度接口。",
  },
  qoder: {
    title: "Qoder 剩余积分",
    source: "Qoder 账号 Credits",
    reason: "本机记录是已上报积分；账号剩余积分需接入已登录的 Qoder Agent SDK。",
  },
  workbuddy: {
    title: "WorkBuddy 剩余积分",
    source: "WorkBuddy 个人账号",
    reason: "本机记录是已上报积分；个人剩余积分需第三方应用授权及积分读取权限。",
  },
}

function percentage(value: number): string {
  const remaining = Math.round((100 - Math.min(100, Math.max(0, value))) * 10) / 10
  return `${remaining}%`
}

export function remainingDisplay(platform: PlatformId, kind: "auth" | "api", quota: QuotaStateDto | null): RemainingDisplay {
  if (platform === "zcode") {
    const base = { title: "ZCode · BigModel 套餐剩余", source: "本机 ZCode BigModel Coding Plan API Key" }
    if (kind === "api") return { ...base, rows: [], state: "unsupported", reason: "当前 API 连接未绑定本机 ZCode BigModel Key。" }
    if (!quota) return { ...base, rows: [], state: "loading", reason: "正在查询 API Key 套餐额度…" }
    if (quota.state !== "ok") return { ...base, rows: [], state: quota.state, reason: quota.reason }
    const rows = quota.windows.map((window) => ({
      label: window.key === "5h" ? "5 小时剩余" : window.key === "7d" ? "周剩余" : "剩余额度",
      value: window.remaining_text ?? (window.used_percent != null && Number.isFinite(window.used_percent) ? percentage(window.used_percent) : "—"),
      usedText: window.amount_text ?? (window.used_percent != null && Number.isFinite(window.used_percent) ? `已用 ${window.used_percent}%` : null),
      resetsAt: window.resets_at,
    }))
    return { ...base, rows, state: rows.some((row) => row.value !== "—") ? "ok" : "unsupported",
      reason: rows.some((row) => row.value !== "—") ? "" : "来源未提供可计算的剩余额度。" }
  }
  if (platform !== "grok") {
    const info = UNAVAILABLE[platform]
    return {
      title: info?.title ?? "剩余额度",
      source: info?.source ?? "在线账号",
      rows: [],
      state: "unsupported",
      reason: info?.reason ?? "当前未接入在线剩余额度查询。",
    }
  }
  if (kind === "api") return {
    title: "xAI API 预付余额",
    source: "xAI 团队钱包",
    rows: [],
    state: "unsupported",
    reason: "此余额需要独立的 Management Key 和团队 ID；API Key 无法读取。",
  }
  const base = { title: "Grok · SuperGrok 剩余额度", source: "本机 Grok OAuth · 订阅额度" }
  if (!quota) return { ...base, rows: [], state: "loading", reason: "正在查询订阅额度…" }
  if (quota.state !== "ok") return { ...base, rows: [], state: quota.state, reason: quota.reason }
  const rows = quota.windows.map((window) => ({
    label: window.key === "7d" ? "SuperGrok 周剩余" : window.key === "30d" ? "SuperGrok 月剩余" : "SuperGrok 剩余",
    value: window.used_percent != null && Number.isFinite(window.used_percent) ? percentage(window.used_percent) : "—",
    usedText: window.amount_text ?? (window.used_percent != null && Number.isFinite(window.used_percent) ? `已用 ${window.used_percent}%` : null),
    resetsAt: window.resets_at,
  }))
  return {
    ...base,
    rows,
    state: rows.some((row) => row.value !== "—") ? "ok" : "unsupported",
    reason: rows.length === 0 || rows.every((row) => row.value === "—") ? "来源未提供可计算的已用百分比。" : "",
  }
}
