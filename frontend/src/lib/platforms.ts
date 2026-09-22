import catalog from "./platformCatalog.json" with { type: "json" }
import type { Platform, PlatformCapability, PlatformId } from "@/types"

/**
 * 平台接入能力的唯一配置源。界面只能按这里声明的能力开放入口，
 * 避免新增平台时仅补一个图标便被误认为已经接入。
 */
export const PLATFORMS: readonly Platform[] = catalog as readonly Platform[]

/** 使用原生来源的平台；Gemini / Grok 另有官方 API Key 只读检测，须同时检查连接 kind */
export const FETCH_ONLY_PLATFORMS: readonly PlatformId[] = ["gemini", "grok", "zcode", "trae", "qoder", "workbuddy"]

export function isFetchOnlyPlatform(id: PlatformId): boolean {
  return FETCH_ONLY_PLATFORMS.includes(id)
}

export function platformConfig(id: PlatformId): Platform {
  return PLATFORMS.find((platform) => platform.id === id) ?? PLATFORMS[0]
}

export function platformSupports(id: PlatformId, capability: PlatformCapability): boolean {
  return platformConfig(id).capabilities.includes(capability)
}

export function platformAvailabilityText(platform: Platform): string {
  if (platform.availability === "available") return "可用"
  if (platform.availability === "not-connected") return "未连接"
  return "待接入"
}
