import { PLATFORMS } from "@/lib/platforms"
import { reducedMotion, useExitPresence } from "@/lib/motion"
import { activeSection, createSpyLock, type SpyLock } from "@/lib/scrollSpyLock"
import { useSlidingIndicator } from "@/lib/useSlidingIndicator"
import { ConnectionPreviews } from "@/components/settings/ConnectionPreviews"
import { CircleArrowUp, Database, Download, LoaderCircle, RefreshCw, Upload, Gauge, Monitor, Moon, Network, Palette, PanelTop, Plug, Settings2, Sun, type LucideIcon } from "lucide-react"
import { SettingIcon } from "@/components/settings/SettingIcon"
import { BalanceAlertSetting } from "@/components/settings/BalanceAlertSetting"
import { SelectField } from "@/components/ui/SelectField"
import { useEffect, useRef, useState } from "react"
import { cn } from "@/lib/utils"
import {
  Button,
  Dropdown,
  Hint,
  Radio,
  SectionTitle,
  Toggle,
} from "@/components/ui/primitives"
import { ConnectionTable } from "@/components/settings/ConnectionTable"
import { useToast } from "@/components/ui/Toast"
import { useUpdate } from "@/components/panel/UpdateContext"
import {
  AddConnectionDialog,
  EditConnectionDialog,
  RemoveConnectionDialog,
} from "@/components/settings/ConnectionDialogs"
import { useConnections } from "@/lib/useConnections"
import type { ProviderPreset } from "@/lib/providerPresets"
import { useSettings } from "@/lib/useSettings"
import { resolveActivePlatform } from "@/lib/connectionPlatformTab.ts"
import { api, type CleanupPreviewDto, type PlanQueryStatusDto, type ProxyStatusDto } from "@/lib/api"
import type { Connection, PlatformId } from "@/types"

function SettingRow({
  label,
  desc,
  control,
}: {
  label: string
  desc?: string
  control: React.ReactNode
}) {
  return (
    <div className="setting-row flex w-full flex-wrap items-center gap-3 rounded-[10px] border bg-surface px-3.5 py-3">
      <SettingIcon label={label} />
      <div className="flex min-w-[160px] flex-1 flex-col gap-[3px]">
        <span className="text-xs font-medium text-text-primary">{label}</span>
        {desc && <span className="text-[11px] leading-[1.4] text-text-tertiary">{desc}</span>}
      </div>
      <div className="setting-control max-w-full shrink-0">{control}</div>
    </div>
  )
}

function SettingsHeading({ icon: Icon, children }: { icon: LucideIcon; children: React.ReactNode }) {
  return <div className="flex min-h-11 items-center gap-2 border-b pb-3">
    <Icon aria-hidden="true" className="size-4 text-accent-blue" strokeWidth={1.75} />
    <SectionTitle>{children}</SectionTitle>
  </div>
}

/** 关于与更新（画布 17 · C）：发现新版后按钮转绿，点击与标题栏绿灯一致直接安装 */
function AboutUpdateRow() {
  const update = useUpdate()
  const toast = useToast()
  const { phase, info, checkNow, startInstall, dismiss } = update
  const checking = phase === "checking"
  const updating = phase === "downloading" || phase === "installing" || phase === "ready"
  const hasUpdate = !!info && (phase === "idle" || phase === "available")

  const onCheck = () => {
    void checkNow()
      .then((available) => toast.success(available ? `发现新版本 v${update.info?.availableVersion ?? ""}` : "已是最新版本"))
      .catch((reason) => toast.danger("检查更新失败", String(reason)))
  }

  return (
    <SettingRow
      label="关于与更新"
      desc={
        hasUpdate && info
          ? `当前版本 v${info.currentVersion} · 发现新版本，点击立即安装`
          : "启动 1 秒后自动检查更新；更新在应用内下载安装并自动重启，期间采集不受影响"
      }
      control={
        updating ? (
          <span className="flex items-center gap-1.5 rounded-[8px] bg-neutral-soft px-3 py-1.5 text-xs text-text-secondary">
            <LoaderCircle className="size-3.5 animate-spin" aria-hidden />
            更新中…
          </span>
        ) : hasUpdate && info ? (
          <span className="flex items-center gap-2">
            <button
              type="button"
              onClick={startInstall}
              className="flex items-center gap-1.5 rounded-[8px] bg-success-soft px-3 py-1.5 text-xs font-semibold text-success-text transition-colors hover:bg-success hover:text-white"
            >
              <CircleArrowUp className="size-3.5" aria-hidden />
              更新到 v{info.availableVersion}
            </button>
            <button
              type="button"
              onClick={() => {
                dismiss()
                toast.success(`已忽略 v${info.availableVersion}，更新版本仍会提示`)
              }}
              className="text-[11px] text-text-tertiary hover:text-text-secondary"
            >
              忽略此版本
            </button>
          </span>
        ) : (
          <button
            type="button"
            onClick={onCheck}
            disabled={checking}
            className="flex items-center gap-1.5 rounded-[8px] bg-neutral-soft px-3 py-1.5 text-xs font-medium text-text-secondary disabled:opacity-60"
          >
            {checking ? <LoaderCircle className="size-3.5 animate-spin" aria-hidden /> : <RefreshCw className="size-3.5" aria-hidden />}
            检查更新
          </button>
        )
      }
    />
  )
}

/** 套餐查询辅助凭证输入框（本地组件；设计令牌与其余设置一致） */
function PlanInput({
  label,
  value,
  onChange,
  placeholder,
  secret = false,
}: {
  label: string
  value: string
  onChange: (v: string) => void
  placeholder: string
  secret?: boolean
}) {
  return (
    <label className="flex min-w-0 flex-1 flex-col gap-1.5">
      <span className="text-xs text-text-secondary">{label}</span>
      <input
        type={secret ? "password" : "text"}
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
        autoComplete="off"
        spellCheck={false}
        className="h-9 w-full rounded-card border bg-surface-2 px-3 text-sm text-text-primary outline-none placeholder:text-text-tertiary focus:border-text-secondary"
      />
    </label>
  )
}

/** 智谱团队版与火山方舟的套餐查询辅助凭证；个人版套餐凭连接自动识别，无需配置 */
function PlanQuerySection() {
  const [status, setStatus] = useState<PlanQueryStatusDto | null>(null)
  const [org, setOrg] = useState("")
  const [project, setProject] = useState("")
  const [akId, setAkId] = useState("")
  const [akSecret, setAkSecret] = useState("")
  const [loaded, setLoaded] = useState(false)
  const [busy, setBusy] = useState(false)
  const toast = useToast()

  useEffect(() => {
    let stopped = false
    void api.getPlanQueryStatus()
      .then((s) => {
        if (stopped) return
        setStatus(s)
        setOrg(s.zhipu_team_organization_id)
        setProject(s.zhipu_team_project_id)
        setAkId(s.volc_access_key_masked ? s.volc_access_key_masked : "")
        setLoaded(true)
      })
      .catch(() => { if (!stopped) setLoaded(true) })
    return () => { stopped = true }
  }, [])

  const save = () => {
    setBusy(true)
    // AK 输入框里是掩码值时传空串，避免把 "****" 当成新值；Secret 留空 = 保持不变
    const akChanged = akId !== (status?.volc_access_key_masked ?? "")
    void api.setPlanQuery({
      zhipuTeamOrganizationId: org,
      zhipuTeamProjectId: project,
      volcAccessKeyId: akChanged ? akId : "",
      volcSecretAccessKey: akSecret === "" ? null : akSecret,
    })
      .then((s) => {
        setStatus(s)
        setOrg(s.zhipu_team_organization_id)
        setProject(s.zhipu_team_project_id)
        setAkId(s.volc_access_key_masked)
        setAkSecret("")
        toast.success("已保存套餐凭证")
      })
      .catch((reason) => toast.danger("套餐凭证保存失败", String(reason)))
      .finally(() => setBusy(false))
  }

  return (
    <section id="settings-plan" className="settings-section flex w-full flex-col gap-4">
      <SettingsHeading icon={Gauge}>套餐查询</SettingsHeading>
      <Hint>
        仅智谱团队版与火山方舟套餐需要在此填写辅助凭证；智谱个人版、Kimi、MiniMax、ZenMux、
        OpenCode Go 按连接的服务地址自动识别，无需任何配置。
      </Hint>
      <section className="flex w-full flex-col gap-3">
        <h3 className="text-xs font-semibold text-text-primary">智谱团队版（open.bigmodel.cn）</h3>
        <div className="flex gap-3">
          <PlanInput label="组织 ID" value={org} onChange={setOrg} placeholder="bigmodel-organization" />
          <PlanInput label="项目 ID" value={project} onChange={setProject} placeholder="bigmodel-project" />
        </div>
        <Hint>两项都填写后按团队套餐查询；都留空则按个人套餐查询。ID 见团队管理后台用量页 URL。</Hint>
      </section>
      <section className="flex w-full flex-col gap-3">
        <h3 className="text-xs font-semibold text-text-primary">火山方舟（Agent / Coding Plan）</h3>
        <div className="flex gap-3">
          <PlanInput label="AccessKey ID" value={akId} onChange={setAkId} placeholder={status?.volc_access_key_masked || "AK…"} />
          <PlanInput label="AccessKey Secret" value={akSecret} onChange={setAkSecret} placeholder={status?.has_volc_secret ? `已设置（尾 4 位 ${status.volc_secret_tail}），留空保持不变` : "与推理 Key 是两套凭证"} secret />
        </div>
        <Hint>火山用量走控制面 OpenAPI 签名，需账号 AK/SK（非推理 API Key）；Secret 留空表示保持已保存的值。</Hint>
      </section>
      <div className="flex items-center gap-3">
        <Button variant="primary" disabled={!loaded || busy} onClick={save}>{busy ? "保存中…" : "保存套餐凭证"}</Button>
      </div>
    </section>
  )
}

/** 统一设置内容，按连接、灵动岛、常规、外观、数据连续排列。 */function SettingsContent({
  onAdd,
  onEdit,
  onRemove,
  conn,
  cfg,
  connectionPlatform,
  onConnectionPlatformChange,
}: {
  onAdd: () => void
  onEdit: (c: Connection) => void
  onRemove: (c: Connection) => void
  conn: ReturnType<typeof useConnections>
  cfg: ReturnType<typeof useSettings>
  /** 连接管理当前查看的平台 */
  connectionPlatform: PlatformId
  onConnectionPlatformChange: (platform: PlatformId) => void
}) {
  const theme = cfg.settings.theme === "system" ? "跟随系统" : cfg.settings.theme === "dark" ? "深色" : "浅色"
  const savePreferences = cfg.setDisplayPreferences
  const [dataInfo, setDataInfo] = useState<{ path: string; size_bytes: number } | null>(null)
  /** 进页面自动发生的读取失败属持续状态，就地说明即可；弹窗只留给用户主动触发的操作 */
  const [dataError, setDataError] = useState<string | null>(null)
  const [autostart, setAutostartState] = useState(false)
  const [autostartLoading, setAutostartLoading] = useState(true)
  /** 只留「初始读不到系统注册项」这一持续状态；切换失败属一次性结果，走 Toast */
  const [autostartError, setAutostartError] = useState<string | null>(null)
  const [cleanup, setCleanup] = useState<(CleanupPreviewDto & { days: number }) | null>(null)
  const [dataBusy, setDataBusy] = useState(false)
  const toast = useToast()
  const [exportPlatform, setExportPlatform] = useState<string>("all")
  useEffect(() => {
    let stopped = false
    void api.dataInfo()
      .then((info) => { if (!stopped) { setDataInfo(info); setDataError(null) } })
      .catch((reason) => { if (!stopped) setDataError(String(reason)) })
    return () => { stopped = true }
  }, [])
  useEffect(() => {
    let stopped = false
    void api.autostartStatus()
      .then((enabled) => {
        if (!stopped) {
          setAutostartState(enabled)
          setAutostartError(null)
        }
      })
      .catch((reason) => { if (!stopped) setAutostartError(String(reason)) })
      .finally(() => { if (!stopped) setAutostartLoading(false) })
    return () => { stopped = true }
  }, [])
  const sizeText = dataInfo
    ? dataInfo.size_bytes < 1024 * 1024
      ? `${(dataInfo.size_bytes / 1024).toFixed(1)} KB`
      : `${(dataInfo.size_bytes / 1024 / 1024).toFixed(1)} MB`
    : "读取中…"
  return (
    <>
      <section id="settings-connections" className="settings-section">
        <ConnectionTable
          connections={conn.connections}
          platform={connectionPlatform}
          onPlatformChange={onConnectionPlatformChange}
          onAdd={onAdd}
          onEdit={onEdit}
          onEnable={cfg.setIslandConnection}
          onApplyToCli={async (id) => {
            const note = await api.enableConnection(id)
            await conn.reload()
            return note
          }}
          onTest={(id) => api.testConnection(id)}
          selectedId={cfg.settings.island_connection_id}
          onRemove={onRemove}
          onPause={conn.setPaused}
          loading={conn.loading}
          error={conn.error}
          onRetry={() => { void conn.reload() }}
        />
      </section>

      <PlanQuerySection />

      <section id="settings-island" className="settings-section flex w-full flex-col gap-4">
        <SettingsHeading icon={PanelTop}>灵动岛</SettingsHeading>
          <section className="flex w-full flex-col gap-3">
            <h3 className="text-xs font-semibold text-text-primary">窗口行为</h3>
            <Hint>
              调整立即应用并在重启后恢复；探出与展开内容使用「大小」，停靠条使用「缩小后大小」。
            </Hint>
            <SettingRow label="灵动岛置顶" desc="保持在其他窗口之上" control={<Toggle on={cfg.settings.always_on_top} onChange={(on) => { void cfg.setTopmost(on) }} label="灵动岛置顶" />} />
            <SettingRow
              label="贴边停靠"
              desc="拖到屏幕边缘自动吸附，关闭时会解除当前停靠"
              control={<Toggle on={cfg.settings.dock_enabled} onChange={(on) => void cfg.setDockEnabled(on)} label="贴边停靠" />}
            />
            <SettingRow
              label="免打扰"
              desc="只暂停 Token 增量提示、水位变色提醒与通知，不停止采集与统计"
              control={
                <Toggle
                  on={cfg.settings.dnd}
                  onChange={(v) => void cfg.setDnd(v)}
                  label="免打扰"
                />
              }
            />
            <SettingRow label="透明度" desc="60%–100%；只影响灵动岛，菜单保持清晰可读" control={<SelectField className="w-[160px]" label="透明度" value={String(cfg.settings.island_opacity)} options={[60, 70, 80, 90, 100].map((n) => ({ value: String(n), label: `${n}%` }))} onValueChange={(v) => void savePreferences({ island_opacity: Number(v) })} />} />
            <SettingRow label="大小" desc="相对系统缩放调整内容大小，窗口随内容重新定位" control={<SelectField className="w-[160px]" label="大小" value={String(cfg.settings.island_scale)} options={[{ value: "85", label: "紧凑 · 85%" }, { value: "100", label: "标准 · 100%" }, { value: "115", label: "大号 · 115%" }]} onValueChange={(v) => void savePreferences({ island_scale: Number(v) })} />} />
            <SettingRow label="缩小后大小" desc="仅缩放停靠条；探出与展开内容仍用「大小」" control={<SelectField className="w-[160px]" label="缩小后大小" value={String(cfg.settings.island_shrink_scale)} options={[{ value: "75", label: "迷你 · 75%" }, { value: "100", label: "标准 · 100%" }, { value: "125", label: "大号 · 125%" }]} onValueChange={(v) => void savePreferences({ island_shrink_scale: Number(v) })} />} />
          </section>

        <section className="flex w-full flex-col gap-3">
          <h3 className="text-xs font-semibold text-text-primary">指标预览</h3>
          <Hint>根据连接数自动扩充，每行两张；仅「使用中」连接驱动灵动岛。</Hint>
          <ConnectionPreviews connections={conn.connections} selectedId={cfg.settings.island_connection_id} />
        </section>
      </section>

      <section id="settings-general" className="settings-section flex w-full flex-col gap-3">
        <SettingsHeading icon={Settings2}>常规</SettingsHeading>
        <SettingRow
          label="开机启动"
          desc={autostartError ?? "随 Windows 登录启动；状态直接读取当前用户的系统注册项"}
          control={
            <Toggle
              on={autostart}
              disabled={autostartLoading}
              label="开机启动"
              onChange={(on) => {
                setAutostartLoading(true)
                setAutostartError(null)
                void api.setAutostart(on)
                  .then((enabled) => {
                    setAutostartState(enabled)
                    // 以后端回读的实际值为准，注册项没写进去时不报喜
                    toast.success(enabled ? "已开启开机启动" : "已关闭开机启动")
                  })
                  .catch((reason) => toast.danger("开机启动设置失败", String(reason)))
                  .finally(() => setAutostartLoading(false))
              }}
            />
          }
        />
        {autostart && <SettingRow
          label="静默启动"
          desc="开机启动时不打开主面板，仍显示灵动岛；关闭后开机自动打开主面板"
          control={<Toggle on={cfg.settings.silent_startup} disabled={autostartLoading} label="静默启动" onChange={(on) => { void cfg.setSilentStartup(on) }} />}
        />}
        <SettingRow
          label="系统托盘"
          desc="关闭主面板不退出应用，采集与灵动岛更新继续运行"
          control={<Dropdown label="固定开启" width={120} />}
        />
        <SettingRow
          label="自动刷新间隔"
          desc="额度、余额与组织用量统一按此间隔刷新；本机会话采集仍实时运行，平台限流时继续退避"
          control={<SelectField className="w-[160px]" label="自动刷新间隔" value={String(cfg.settings.refresh_minutes)} options={[1, 5, 15, 30].map((n) => ({ value: String(n), label: `${n} 分钟` }))} onValueChange={(v) => void savePreferences({ refresh_minutes: Number(v) })} />}
        />
        <BalanceAlertSetting settings={cfg.settings} />
        <SettingRow label="语言" desc="当前仅支持简体中文，暂不提供语言切换" control={<Dropdown label="简体中文" width={160} />} />
        <SettingRow
          label="统计时区"
          desc="首期按操作系统本地时区计算今日、本周、本月与日志日期"
          control={<Dropdown label={Intl.DateTimeFormat().resolvedOptions().timeZone || "系统本地时区"} width={160} />}
        />
        <AboutUpdateRow />
      </section>

      <section id="settings-proxy" className="settings-section flex w-full flex-col gap-3">
        <SettingsHeading icon={Network}>代理</SettingsHeading>
        <ProxySection cfg={cfg} />
      </section>

      <section id="settings-appearance" className="settings-section flex w-full flex-col gap-3">
        <SettingsHeading icon={Palette}>外观</SettingsHeading>
        <Hint>
          主面板与灵动岛同步切换并记忆选择；切换主题时平台 / 账号 / 来源、查询范围与会话状态保持不变。
        </Hint>
        <div className="flex w-full gap-3">
          {[
            ["浅色", "白色背景、细边框、绿色进度条"],
            ["深色", "近黑背景、稍浅卡片、相同组件结构"],
            ["跟随系统", "随系统浅色 / 深色自动切换"],
          ].map(([name, note]) => (
            <button
              key={name}
              type="button"
              aria-pressed={theme === name}
              onClick={() => {
                void cfg.setTheme(name === "跟随系统" ? "system" : name === "深色" ? "dark" : "light")
              }}
              className={cn(
                "motion-button flex min-w-0 flex-1 flex-col gap-2 rounded-xl border bg-surface p-3.5 text-left",
                theme === name ? "border-[1.5px] border-accent-blue" : "hover:border-border-strong",
              )}
            >
              <span className="flex items-center gap-2">
                {name === "浅色" ? <Sun aria-hidden="true" className="size-4 text-warn" /> : name === "深色" ? <Moon aria-hidden="true" className="size-4 text-purple-text" /> : <Monitor aria-hidden="true" className="size-4 text-accent-blue" />}
                <Radio checked={theme === name} />
                <span className="text-xs font-semibold text-text-primary">{name}</span>
              </span>
              <span className="text-[11px] leading-[1.5] text-text-tertiary">{note}</span>
            </button>
          ))}
        </div>
      </section>

      <section id="settings-data" className="settings-section flex w-full flex-col gap-3">
        <SettingsHeading icon={Database}>数据</SettingsHeading>
        {/* 数据库位置已确认改为只读：只显示路径与占用，首期不提供迁移 */}
        <div className="flex w-full flex-wrap items-center gap-3 rounded-[10px] border bg-surface px-3.5 py-3">
          <SettingIcon label="数据库位置" />
          <div className="flex min-w-0 flex-1 flex-col gap-[3px]">
            <span className="text-xs font-medium text-text-primary">数据库位置</span>
            <span className={cn("tnum break-all font-mono text-[11px] leading-[1.4]", dataError ? "text-danger" : "text-text-secondary")}>
              {dataInfo?.path ?? (dataError ? `数据库信息读取失败：${dataError}` : "正在读取实际数据库路径")}&nbsp;&nbsp;·&nbsp;&nbsp;占用 {sizeText}
            </span>
            <span className="text-[11px] leading-[1.4] text-text-tertiary">
              SQLite 按查询范围读取，避免全部历史常驻内存；首期不提供数据库迁移，如需更换位置请手动迁移后重启应用。
            </span>
          </div>
          <Button onClick={() => {
            void api.openDataDirectory().catch((reason) => toast.danger("打开数据目录失败", String(reason)))
          }}>打开所在文件夹</Button>
        </div>
        <SettingRow
          label="导入 / 导出"
          desc="导入打开系统文件选择器；导出选择保存位置。JSON 仅包含统计记录与会话标题，不含连接或凭证；导入自动去重"
          control={
            <div className="flex items-center gap-2">
              <SelectField
                label="导出范围"
                value={exportPlatform}
                disabled={dataBusy}
                onValueChange={(value) => setExportPlatform(value as typeof exportPlatform)}
                className="w-[120px]"
                options={[{ value: "all", label: "全部平台" }, ...PLATFORMS.map(p => ({ value: p.id, label: p.name }))]}
              />
              <Button disabled={dataBusy} onClick={() => {
                setDataBusy(true)
                void api.importDataFile().then(async (result) => {
                  // 用户在系统文件选择器里点了取消，什么都没发生，不该弹提示
                  if (!result) return
                  toast.success("导入完成", `更新 ${result.requests_changed.toLocaleString("zh-CN")} 条请求、${result.sessions_changed.toLocaleString("zh-CN")} 条会话标题`)
                  setCleanup(null)
                  await api.dataInfo().then(setDataInfo).catch(reason => toast.warn("数据库容量刷新失败", String(reason)))
                }).catch(reason => toast.danger("导入失败", String(reason))).finally(() => setDataBusy(false))
              }}><Upload aria-hidden className="size-3" />导入</Button>
              <Button
                disabled={dataBusy}
                onClick={() => {
                  setDataBusy(true)
                  void api.exportDataFile(exportPlatform === "all" ? null : exportPlatform)
                    .then((path) => { if (path) toast.success("导出完成", path) })
                    .catch((reason) => toast.danger("导出失败", String(reason)))
                    .finally(() => setDataBusy(false))
                }}
              ><Download aria-hidden className="size-3" />导出</Button>
            </div>
          }
        />
        <SettingRow
          label="历史清理"
          desc="默认不自动删除；清理会保留采集偏移，旧日志不会在下一轮重新补采"
          control={
            <div className="flex items-center gap-2">
              <SelectField
                label="历史保留时间"
                value={String(cfg.settings.retention_days ?? "")}
                disabled={dataBusy}
                onValueChange={(value) => {
                  const days = value ? Number(value) : null
                  setCleanup(null)
                  // 新值下拉框里就写着，成功不必再弹；保存失败由顶部 cfg.error 红条播报，
                  // 那条红条是 sticky 的且带「重新读取」，再弹 Toast 只是重复出声
                  void cfg.setRetentionDays(days)
                }}
                className="w-[140px]"
                options={[
                  { value: "", label: "不自动清理" },
                  ...[30, 90, 180, 365].map((days) => ({ value: String(days), label: `保留 ${days} 天` })),
                ]}
              />
              <Button
                disabled={dataBusy || cfg.settings.retention_days === null}
                onClick={() => {
                  const days = cfg.settings.retention_days
                  if (days === null) return
                  setDataBusy(true)
                  void api.cleanupPreview(days)
                    .then((preview) => setCleanup({ ...preview, days }))
                    .catch((reason) => toast.danger("清理预览失败", String(reason)))
                    .finally(() => setDataBusy(false))
                }}
              >{dataBusy ? "处理中…" : "预览"}</Button>
            </div>
          }
        />
        {cleanup && (
          <div role="alertdialog" aria-label="确认历史清理" className="flex items-center gap-3 rounded-[10px] border border-warn bg-warn-soft px-3.5 py-3">
            <p className="min-w-0 flex-1 text-[11px] leading-[1.5] text-warn">
              将删除 {cleanup.requests.toLocaleString("zh-CN")} 条请求、{cleanup.usage_events.toLocaleString("zh-CN")} 条增量事件和 {cleanup.sessions.toLocaleString("zh-CN")} 条已结束会话信息。此操作不可撤销。
            </p>
            <Button disabled={dataBusy} onClick={() => setCleanup(null)}>取消</Button>
            <Button
              variant="danger"
              disabled={dataBusy}
              onClick={() => {
                const days = cleanup.days
                setDataBusy(true)
                void api.cleanupHistory(days)
                  .then((result) => {
                    setCleanup(null)
                    toast.success(`已清理 ${result.requests.toLocaleString("zh-CN")} 条请求记录`, `保留最近 ${days} 天；采集偏移保留，旧日志不会重新补采`)
                    return api.dataInfo()
                  })
                  .then(setDataInfo)
                  .catch((reason) => toast.danger("历史清理失败", String(reason)))
                  .finally(() => setDataBusy(false))
              }}
            >确认清理</Button>
          </div>
        )}
      </section>
    </>
  )
}

/** 本地代理（阶段二）：CLI 网关流量经本机代理转发并计时「用时 / 首字」；官方订阅直连暂不支持接管 */
function ProxySection({ cfg }: { cfg: ReturnType<typeof useSettings> }) {
  const [status, setStatus] = useState<ProxyStatusDto | null>(null)
  const [portDraft, setPortDraft] = useState(String(cfg.settings.proxy_port))
  const [busy, setBusy] = useState(false)
  const toast = useToast()

  // 状态随开关 / 端口变化刷新；保存失败时 cfg.error 顶部红条已有展示
  useEffect(() => {
    let stopped = false
    void api.proxyStatus()
      .then((s) => { if (!stopped) setStatus(s) })
      .catch(() => {})
    return () => { stopped = true }
  }, [cfg.settings.proxy_enabled, cfg.settings.proxy_port])

  useEffect(() => { setPortDraft(String(cfg.settings.proxy_port)) }, [cfg.settings.proxy_port])

  const portNumber = Number(portDraft)
  const portValid = Number.isInteger(portNumber) && portNumber >= 1024 && portNumber <= 65535
  const portChanged = portNumber !== cfg.settings.proxy_port
  // 开关与回退开关都按当前端口草稿提交：改完端口直接切开关，端口一并生效
  const applyPort = portValid ? portNumber : cfg.settings.proxy_port

  const apply = (patch: { enabled?: boolean; fallback?: boolean }) => {
    if (busy) return
    setBusy(true)
    const enabled = patch.enabled ?? cfg.settings.proxy_enabled
    const fallback = patch.fallback ?? cfg.settings.proxy_fallback_direct
    void cfg.setProxy(applyPort, enabled, fallback)
      .then((ok) => {
        // 失败由 cfg.error 顶部红条播报，这里不重复出声
        if (!ok) return
        if (patch.enabled !== undefined) {
          if (enabled) toast.success(`本地代理已启用 · 127.0.0.1:${applyPort}`, "CLI 网关流量经本机转发并记录用时 / 首字")
          else toast.success("已停止本地代理", "CLI 配置已还原直连")
        } else if (patch.fallback !== undefined) {
          toast.success(fallback ? "已开启失败自动回退直连" : "已关闭失败自动回退直连")
        }
      })
      .finally(() => {
        setBusy(false)
        void api.proxyStatus().then(setStatus).catch(() => {})
      })
  }

  const upstreamNote = (() => {
    if (!cfg.settings.proxy_enabled) return "未启用：请求日志的用时 / 首字显示「—」，采集与统计不受影响"
    if (status?.fallback_reason) return `已自动回退直连：${status.fallback_reason}`
    if (!status?.running) return "正在启动监听…"
    const parts: string[] = []
    if (status.claude_taken_over) parts.push(`Claude → ${status.claude_upstream}`)
    else parts.push("Claude 未接管（官方直连或环境变量上游暂不支持）")
    if (status.codex_taken_over) parts.push(`Codex → ${status.codex_upstream}`)
    else parts.push("Codex 未接管（官方订阅暂不支持）")
    return `运行中 · 127.0.0.1:${status.port} · ${parts.join("；")}`
  })()

  return (
    <>
      <Hint>
        开启后 CLI 的网关流量经本机代理转发并记录「用时 / 首字」；本机配置会临时指向
        127.0.0.1，关闭或退出应用时自动还原直连。官方订阅直连暂不支持接管，不影响其原有行为。
      </Hint>
      <SettingRow
        label="启用本地代理"
        desc={upstreamNote}
        control={
          <Toggle
            on={cfg.settings.proxy_enabled}
            disabled={busy}
            label="启用本地代理"
            onChange={(on) => apply({ enabled: on })}
          />
        }
      />
      <SettingRow
        label="监听端口"
        desc="仅监听 127.0.0.1；修改端口会重新接管并重启监听"
        control={
          <input
            type="text"
            inputMode="numeric"
            aria-label="监听端口"
            value={portDraft}
            disabled={busy}
            onChange={(e) => setPortDraft(e.target.value.replace(/[^\d]/g, ""))}
            onBlur={() => { if (!portValid) setPortDraft(String(cfg.settings.proxy_port)) }}
            autoComplete="off"
            spellCheck={false}
            aria-invalid={!portValid}
            className={cn(
              "h-9 w-[120px] rounded-card border bg-surface-2 px-3 text-sm text-text-primary outline-none focus:border-text-secondary",
              !portValid && "border-danger",
            )}
          />
        }
      />
      <SettingRow
        label="失败自动回退直连"
        desc="上游连续 3 次不可达时，自动还原 CLI 直连配置并停止代理；统计缺失优先于 CLI 不可用"
        control={
          <Toggle
            on={cfg.settings.proxy_fallback_direct}
            disabled={busy}
            label="失败自动回退直连"
            onChange={(on) => apply({ fallback: on })}
          />
        }
      />
      {status?.last_error && cfg.settings.proxy_enabled && (
        <p role="status" className="text-[11px] text-warn">最近转发错误：{status.last_error}</p>
      )}
      {portChanged && (
        <p className="text-[11px] text-text-tertiary">
          端口 {portValid ? portNumber : "无效"}尚未生效：{portValid ? "切换任一开关或重启代理后按新端口接管" : "须为 1024–65535 的整数"}
        </p>
      )}
    </>
  )
}

const SETTINGS_SECTIONS = [
  { id: "connections", label: "连接管理", icon: Plug },
  { id: "plan", label: "套餐查询", icon: Gauge },
  { id: "island", label: "灵动岛", icon: PanelTop },
  { id: "general", label: "常规", icon: Settings2 },
  { id: "proxy", label: "代理", icon: Network },
  { id: "appearance", label: "外观", icon: Palette },
  { id: "data", label: "数据", icon: Database },
] as const

export type SettingsSection = typeof SETTINGS_SECTIONS[number]["id"]

/** 分区结构是常量，底板的监听因此只绑定一次 */
const SECTIONS_KEY = SETTINGS_SECTIONS.map((item) => item.id).join(",")

export function SettingsView({
  section,
  openAddRequest = 0,
  navigationRequest = 0,
}: {
  section: SettingsSection
  /** 由所选连接驱动，与总览的当前查看平台相互独立 */
  islandPlatform: PlatformId
  openAddRequest?: number
  navigationRequest?: number
}) {
  const [dialog, setDialog] = useState<null | { kind: "add"; platform: PlatformId; provider?: ProviderPreset | null } | { kind: "edit"; c: Connection } | { kind: "remove"; c: Connection }>(null)
  const { rendered: shownDialog, exiting: dialogExiting } = useExitPresence(dialog)
  const conn = useConnections()
  const cfg = useSettings()
  const toast = useToast()

  /**
   * 连接管理按平台分页。数据就绪后停在灵动岛使用中的连接所属平台，之后交给用户手动切换；
   * 不做记忆，重新打开设置仍按当时使用中的连接解析。
   */
  const [connectionPlatform, setConnectionPlatform] = useState<PlatformId>("claude")
  const platformResolved = useRef(false)
  useEffect(() => {
    if (platformResolved.current || conn.loading || !cfg.ready) return
    platformResolved.current = true
    setConnectionPlatform(resolveActivePlatform({
      connections: conn.connections,
      selectedId: cfg.settings.island_connection_id,
    }))
  }, [conn.loading, conn.connections, cfg.ready, cfg.settings.island_connection_id])

  useEffect(() => { if (openAddRequest > 0) setDialog({ kind: "add", platform: connectionPlatform }) }, [openAddRequest])

  const rootRef = useRef<HTMLDivElement>(null)
  const [active, setActive] = useState<SettingsSection>(section)
  /** 点击导航后把高亮钉在目标上，等滚动停下再交还给滚动推导 */
  const spyLock = useRef<SpyLock>(null)
  // 导航的选中胶囊与顶部「总览 / 设置」共用同一套滑动底板
  const { rootRef: navRef, plateRef: navIndicator } = useSlidingIndicator<HTMLElement, HTMLSpanElement>({
    selector: 'button[aria-current="location"]',
    itemsKey: SECTIONS_KEY,
    value: active,
  })

  /**
   * @param smooth 用户点导航时为 true：平滑滚过去，看得见自己去了哪。
   *   托盘 / 程序化定位仍瞬时到位——那是定位，不是表演，中间的分区不该被"路过"。
   */
  const jumpTo = (id: SettingsSection, smooth = false) => {
    if (rootRef.current && navRef.current) rootRef.current.style.setProperty("--settings-scroll-offset", `${navRef.current.offsetHeight + 16}px`)
    spyLock.current = createSpyLock(id, Date.now())
    rootRef.current?.querySelector<HTMLElement>(`#settings-${id}`)?.scrollIntoView({
      block: "start",
      behavior: smooth && !reducedMotion() ? "smooth" : "instant",
    })
    setActive(id)
  }

  useEffect(() => { jumpTo(section) }, [section, navigationRequest, openAddRequest])

  useEffect(() => {
    const root = rootRef.current
    const scroller = root?.closest<HTMLElement>("[data-panel-scroll]")
    if (!root || !scroller) return
    let frame = 0
    /**
     * 分区节点按 id 缓存：原先每帧对 7 个分区各做一次 querySelector，
     * 滚动时白白重复整棵子树的选择器匹配。
     */
    const nodes = new Map<SettingsSection, HTMLElement>()
    const nodeOf = (id: SettingsSection) => {
      const cached = nodes.get(id)
      if (cached?.isConnected) return cached
      const found = root.querySelector<HTMLElement>(`#settings-${id}`)
      if (found) nodes.set(id, found)
      return found
    }
    /** 上一次写进 CSS 变量的值；没变就不写，写了会让下一次读布局被迫同步重排 */
    let lastOffset = -1
    const update = () => {
      // ——— 先集中读，一次布局都不多做 ———
      const navHeight = navRef.current?.offsetHeight ?? 0
      const boundary = (navRef.current?.getBoundingClientRect().bottom ?? scroller.getBoundingClientRect().top) + 17
      let current: SettingsSection = "connections"
      for (const item of SETTINGS_SECTIONS) {
        const node = nodeOf(item.id)
        if (node && node.getBoundingClientRect().top <= boundary) current = item.id
      }
      // ——— 读完了再写 ———
      // 原先每帧先写这个变量再读 rect，等于每帧强制同步重排一次（trace 实测 14ms）
      const offset = navHeight + 16
      if (navHeight && offset !== lastOffset) {
        lastOffset = offset
        root.style.setProperty("--settings-scroll-offset", `${offset}px`)
      }
      // 平滑滚动途中会连续扫过中间分区；锁还在就别让高亮一路追着跑
      setActive(activeSection(spyLock.current, current, Date.now()) as SettingsSection)
    }
    let idle = 0
    const onScroll = () => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(update)
      // 滚动停稳 120ms 即解锁，把高亮交还给推导
      window.clearTimeout(idle)
      idle = window.setTimeout(() => { spyLock.current = null; update() }, 120)
    }
    scroller.addEventListener("scroll", onScroll, { passive: true })
    window.addEventListener("resize", onScroll)
    onScroll()
    return () => {
      cancelAnimationFrame(frame)
      window.clearTimeout(idle)
      scroller.removeEventListener("scroll", onScroll)
      window.removeEventListener("resize", onScroll)
    }
  }, [])

  return (
    <div ref={rootRef} className="unified-settings relative flex w-full flex-col gap-7 px-4 pb-8 sm:px-8">
      <nav ref={navRef} aria-label="设置分区" className="sticky top-0 z-10 -mx-4 flex gap-1 overflow-x-auto border-b bg-bg px-4 py-3 isolate sm:-mx-8 sm:gap-2 sm:px-8">
        <span ref={navIndicator} aria-hidden="true" data-motion-indicator className="motion-indicator pointer-events-none absolute left-0 top-0 -z-10 rounded-lg bg-accent-blue-soft" />
        {SETTINGS_SECTIONS.map(({ id, label, icon: Icon }) => (
          <button key={id} type="button" aria-current={active === id ? "location" : undefined}
            onClick={() => jumpTo(id, true)}
            className={cn("relative flex shrink-0 items-center gap-2 rounded-lg px-3 py-2.5 text-xs transition-colors hover:text-text-primary focus-visible:outline-2 focus-visible:outline-accent-blue", active === id ? "font-semibold text-accent-blue" : "text-text-secondary")}>
            <Icon aria-hidden="true" className="size-4" strokeWidth={1.75} />{label}
          </button>
        ))}
      </nav>
      {cfg.error && <div role="alert" className="sticky top-[68px] z-10 flex items-center justify-between gap-3 rounded-lg border border-danger bg-danger-soft px-3 py-2 text-xs text-danger">
        <span>{cfg.error}</span><Button disabled={cfg.loading || cfg.saving} onClick={() => void cfg.reload()}>重新读取</Button>
      </div>}
      {cfg.loading && <p role="status" className="text-xs text-text-secondary">正在读取设置…</p>}
      <fieldset data-saving={cfg.saving} disabled={!cfg.ready || cfg.loading || cfg.saving} aria-busy={cfg.loading || cfg.saving} className="flex min-w-0 flex-col gap-7 border-0 p-0">
      <SettingsContent
        onAdd={() => setDialog({ kind: "add", platform: connectionPlatform })}
        onEdit={(c) => setDialog({ kind: "edit", c })}
        onRemove={(c) => setDialog({ kind: "remove", c })}
        conn={conn}
        cfg={cfg}
        connectionPlatform={connectionPlatform}
        onConnectionPlatformChange={setConnectionPlatform}
      />
      </fieldset>

      {shownDialog && (
        <div inert={dialogExiting} data-state={dialogExiting ? "closed" : "open"} className="motion-overlay fixed inset-0 z-50 grid place-items-center overflow-y-auto bg-black/25 p-4 sm:p-8">
            {shownDialog.kind === "add" ? <AddConnectionDialog exiting={dialogExiting} initialPlatform={shownDialog.platform} initialProvider={shownDialog.provider ?? null} onClose={() => setDialog(null)} onRead={conn.readLocal} onAddManual={async (input) => { await conn.add({ ...input, kind: "api" }) }} /> : shownDialog.kind === "edit" ? <EditConnectionDialog
              connection={shownDialog.c}
              exiting={dialogExiting}
              onClose={() => setDialog(null)}
              onSubmit={async (input) => {
                // 后端统一快照式更新：地址或 Key 变化时先验证再落库，失败整体报错
                await api.updateConnection(input)
              }}
            /> : <RemoveConnectionDialog
              exiting={dialogExiting}
              connection={shownDialog.c}
              /* 被移除的连接正是灵动岛当前配置时，才显示橙色警示（§6.3） */
              usedByIsland={shownDialog.c.id === cfg.settings.island_connection_id}
              onClose={() => setDialog(null)}
              onConfirm={async () => {
                await conn.remove(shownDialog.c.id)
                toast.success(`已移除 ${shownDialog.c.name}`, "配置与凭证已删除，历史统计保留")
                setDialog(null)
              }}
            />}
        </div>
      )}
    </div>
  )
}
