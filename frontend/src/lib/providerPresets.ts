import type { PlatformId } from "@/types"

import type { ProviderIconId } from "@/components/brand/ProviderLogo"

/**
 * 供应商预设（画布 18，鼠鼠需求）。
 *
 * 本质是"endpoint + 协议"的快捷模板：点标签 = 打开添加连接弹窗并预填
 * Base URL（用户只需粘贴 Key），连接仍归属 Claude / Codex 平台（CLI 维度），
 * 数据库与统计结构不变。base_url 模板来自各官方文档，填写后可自由修改。
 */
export interface ProviderPreset {
  /** 与 ProviderLogo 的图标一一对应（自定义 = 通用滑杆图标） */
  id: ProviderIconId
  name: string
  /** 归属的 CLI 平台：决定写谁的配置（Anthropic 兼容 → claude，OpenAI 兼容 → codex） */
  platform: PlatformId
  /** 兼容端点模板；空串表示官方地址未确认，由用户自行填写 */
  baseUrl: string
}

export const PROVIDER_PRESETS: ProviderPreset[] = [
  // 自定义固定置顶（鼠鼠需求）：最常见的"任意网关手填"路径排在第一个
  { id: "custom", name: "自定义", platform: "claude", baseUrl: "" },
  { id: "glm", name: "GLM 智谱", platform: "claude", baseUrl: "https://open.bigmodel.cn/api/anthropic" },
  { id: "kimi", name: "Kimi", platform: "claude", baseUrl: "https://api.moonshot.cn/anthropic" },
  { id: "deepseek", name: "DeepSeek", platform: "claude", baseUrl: "https://api.deepseek.com/anthropic" },
  { id: "minimax", name: "MiniMax", platform: "claude", baseUrl: "https://api.minimaxi.com/anthropic" },
  { id: "volc", name: "火山方舟", platform: "claude", baseUrl: "https://ark.cn-beijing.volces.com/api/anthropic" },
  { id: "qwen", name: "Qwen 通义", platform: "claude", baseUrl: "" },
  { id: "mimo", name: "小米 MiMo", platform: "claude", baseUrl: "" },
  { id: "siliconflow", name: "硅基流动", platform: "codex", baseUrl: "https://api.siliconflow.cn/v1" },
  { id: "openrouter", name: "OpenRouter", platform: "codex", baseUrl: "https://openrouter.ai/api/v1" },
]
