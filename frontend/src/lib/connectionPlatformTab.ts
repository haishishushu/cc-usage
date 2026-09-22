import { PLATFORMS, platformSupports } from "./platforms.ts"
import type { PlatformId } from "@/types"

/** 连接管理按平台分页所需的最小连接字段，便于单测直接构造。 */
export type PlatformTabConnection = { id: string; platformId: PlatformId }

/** 平台是否声明了连接能力；只有额度能力的平台（如 Grok）不开放添加入口。 */
export function canAddConnection(platform: PlatformId): boolean {
  return platformSupports(platform, "auth_connection") || platformSupports(platform, "api_connection")
}

function selectable(id: string | null | undefined): PlatformId | null {
  const platform = PLATFORMS.find((item) => item.id === id)
  return platform && platform.availability !== "not-integrated" ? platform.id : null
}

/**
 * 决定连接管理停在哪个平台：灵动岛正在使用的连接所属平台，没有则 Claude。
 * 不做记忆，每次按当前使用中的连接解析。
 */
export function resolveActivePlatform({
  connections,
  selectedId,
}: {
  connections: readonly PlatformTabConnection[]
  selectedId: string | null | undefined
}): PlatformId {
  const using = connections.find((connection) => connection.id === selectedId)
  return selectable(using?.platformId) ?? "claude"
}
