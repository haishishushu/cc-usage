import { IslandCollapsed, IslandExpanded, type IslandData } from "@/components/island/UsageIsland"
import { TrayMenu, TrayPlatformSubmenu, TrayTooltip } from "@/components/tray/TrayMenu"
import { RequestLogTable } from "@/components/panel/RequestLogTable"
import { Pagination } from "@/components/panel/Pagination"
import {
  API_SOURCE,
  API_TODAY,
  CLAUDE_QUOTAS,
  CODEX_QUOTAS,
  LOG_PAGE_COUNT,
  LOG_ROWS,
  LOG_TOTAL_COUNT,
  SESSIONS,
  TOTAL_DELTA_TEXT,
  TRAY_TOOLTIP,
} from "@/mock/data"


function Section({ title, desc, children }: { title: string; desc?: string; children: React.ReactNode }) {
  return (
    <section className="flex w-full flex-col gap-4">
      <div className="flex flex-col gap-1">
        <h2 className="text-base font-semibold text-text-primary">{title}</h2>
        {desc && <p className="text-xs leading-[1.6] text-text-secondary">{desc}</p>}
      </div>
      <div className="flex flex-wrap items-start gap-6">{children}</div>
    </section>
  )
}

export default function Showcase() {
  const authData: IslandData = {
    platform: "claude",
    platformName: "Claude",
    kind: "auth",
    quotas: CLAUDE_QUOTAS,
    sessionCountText: "3 个会话运行中",
    deltaText: TOTAL_DELTA_TEXT,
    sessions: SESSIONS,
    todayTokenText: "12.84M",
    status: { tone: "success", label: "已连接" },
    sourceText: "统计来源：本地会话记录（本机 Claude CLI 记录）· 最近更新 12 秒前",
  }
  const apiData: IslandData = {
    platform: "claude",
    platformName: "Claude",
    kind: "api",
    apiToday: { token: API_TODAY.token, cost: API_TODAY.cost },
    connectionLabel: "个人 API Key · sk-ant-****3f9a",
    sessionCountText: "3 个会话运行中",
    deltaText: TOTAL_DELTA_TEXT,
    sessions: SESSIONS,
    sourceText: `统计来源：${API_SOURCE.name}（${API_SOURCE.scope}）· 请求结束后更新 · 最近更新 ${API_SOURCE.lastUpdatedText}`,
  }

  return (
    <div className="flex w-full max-w-[1200px] flex-col gap-12 p-10">
      <Section title="01 · 灵动岛 Auth 收缩态" desc="收缩态不显示「打开主面板」按钮；右侧 112px 为增量固定预留区。">
        <IslandCollapsed data={authData} />
      </Section>

      <Section title="02 · 收缩态变体" desc="Codex 双额度 / 缺少 5h 数据时保留长横线 / 未连接。">
        <IslandCollapsed
          data={{ ...authData, platform: "codex", platformName: "Codex", quotas: CODEX_QUOTAS, sessionCountText: null, deltaText: "+2.4k Token" }}
        />
        <IslandCollapsed
          data={{ ...authData, platform: "codex", platformName: "Codex", quotas: [CODEX_QUOTAS[1]], sessionCountText: null, deltaText: "+860 Token" }}
        />
        <IslandCollapsed
          data={{
            ...authData,
            sessionCountText: null,
            deltaText: null,
            unavailable: { title: "连接已过期", hint: "额度不可用 · 双击展开后打开主面板修复", tone: "warn" },
          }}
        />
      </Section>

      <Section title="03 · 灵动岛 Auth 展开态" desc="右上角是灵动岛内部唯一的入口；会话列表最多 3 条。">
        <IslandExpanded data={authData} />
      </Section>

      <Section title="04 / 05 · API 灵动岛" desc="今日 Token 与今日费用；余额只在主面板查看。">
        <IslandCollapsed data={apiData} />
        <IslandExpanded data={apiData} />
      </Section>

      <Section title="11 · 请求日志状态" desc="骨架 / 空 / 失败 / 来源不支持，以及五种分页状态。">
        <div className="flex w-full flex-col gap-6">
          <RequestLogTable rows={[]} state="loading" />
          <RequestLogTable rows={[]} state="empty" />
          <RequestLogTable rows={[]} state="failed" />
          <RequestLogTable rows={[]} state="unsupported" />
          <RequestLogTable rows={LOG_ROWS} />
          <div className="flex flex-col gap-3">
            <Pagination current={1} pageCount={LOG_PAGE_COUNT} totalCount={LOG_TOTAL_COUNT} />
            <Pagination current={9} pageCount={LOG_PAGE_COUNT} totalCount={LOG_TOTAL_COUNT} />
            <Pagination current={17} pageCount={LOG_PAGE_COUNT} totalCount={LOG_TOTAL_COUNT} />
            <Pagination current={3} pageCount={6} totalCount={58} />
            <Pagination current={1} pageCount={1} totalCount={null} />
          </div>
        </div>
      </Section>

      <Section title="18 · 系统托盘" desc="左键单击 = 显示 / 隐藏灵动岛；右键 = 弹出菜单。">
        <TrayTooltip
          app={TRAY_TOOLTIP.app}
          connection={TRAY_TOOLTIP.connection}
          quotas={TRAY_TOOLTIP.quotas}
          updated={TRAY_TOOLTIP.updated}
        />
        <TrayMenu />
        <TrayPlatformSubmenu />
        <TrayMenu state={{ refreshing: true }} />
        <TrayMenu state={{ dnd: true }} />
        <TrayMenu state={{ islandHidden: true }} />
      </Section>
    </div>
  )
}

