import { useEffect, useId, useRef, useState } from "react"
import { LoaderCircle, RefreshCw, ShieldCheck, TriangleAlert, X, Eye, EyeOff } from "lucide-react"
import { cn } from "@/lib/utils"
import { PlatformLogo } from "@/components/brand/PlatformLogo"
import { ProviderLogo } from "@/components/brand/ProviderLogo"
import { Chip, Radio, Toggle } from "@/components/ui/primitives"
import { SelectField } from "@/components/ui/SelectField"
import { useToast } from "@/components/ui/Toast"
import { PLATFORMS, platformSupports, isFetchOnlyPlatform } from "@/lib/platforms"
import { planConnectionEdit } from "@/lib/connectionEdit"
import { api, isTauri } from "@/lib/api"
import type { Connection, PlatformId } from "@/types"
import type { ProviderPreset } from "@/lib/providerPresets"
import { PROVIDER_PRESETS } from "@/lib/providerPresets"

/** 思考强度枚举：与请求日志 effort 列同口径，界面直接以英文展示（即 API 参数原值） */
const EFFORT_KEYS = ["off", "none", "minimal", "low", "medium", "high", "xhigh", "max"]
/** 添加与编辑弹窗统一宽度：主窗口的 80%，随窗口大小自动缩放（鼠鼠需求） */
const DIALOG_WIDTH = "80%"

/**
 * 思考强度选项（画布 18，鼠鼠需求）：随所选模型从数据库规则拉取档位，
 * 「获取强度」按钮调官方 /v1/models 解析每个模型的 capabilities.effort 并存入本机
 * 数据库——官方加新档位/新模型时点一下即可选到，无需发版。非 Tauri 预览回落内置枚举。
 */
function useEffortChoices(model: string, platform: string, baseUrl: string, secret: string) {
  const toast = useToast()
  const [effortOptions, setEffortOptions] = useState(EFFORT_KEYS.map((value) => ({ value, label: value })))
  const [fetchingEfforts, setFetchingEfforts] = useState(false)
  useEffect(() => {
    if (!isTauri) return
    const timer = window.setTimeout(() => {
      api.getEffortOptions(model)
        .then((list) => { if (list.length) setEffortOptions(list.map((value) => ({ value, label: value }))) })
        .catch(() => {})
    }, 250)
    return () => window.clearTimeout(timer)
  }, [model])
  const refreshEfforts = async () => {
    setFetchingEfforts(true)
    try {
      if (!baseUrl.trim()) throw new Error("请先填写 Base URL")
      if (!secret.trim()) throw new Error("请先填写 API Key")
      const note = await api.refreshEffortLevels(platform, baseUrl.trim(), secret.trim())
      const list = await api.getEffortOptions(model)
      if (list.length) setEffortOptions(list.map((value) => ({ value, label: value })))
      toast.success(note)
    } catch (reason) {
      const message = String(reason)
      // 网关不提供 /v1/models（404）或未透传强度元数据时，指引用户回落内置规则
      if (message.includes("404") || message.includes("capabilities")) {
        toast.warn("网关未提供强度元数据", "该网关不支持 /v1/models 或未透传 capabilities；已内置 2026-09 查证的档位规则可继续使用")
      } else {
        toast.danger("获取强度失败", message)
      }
    } finally { setFetchingEfforts(false) }
  }
  return { effortOptions, fetchingEfforts, refreshEfforts }
}

/** 登录在用户终端完成；应用仅在用户确认后重新读取本机凭证。 */
export function CliAuthDialog({ connection, onClose, onRename, onUpdated, exiting = false }: {
  connection: Connection
  onClose: () => void
  /** 名称修改随主按钮一并保存；读取凭证失败不影响已保存的名称 */
  onRename?: (name: string) => Promise<void>
  onUpdated: () => Promise<void>
  exiting?: boolean
}) {
  const command = connection.platformId === "claude" ? "claude auth login" : "codex login"
  const [name, setName] = useState(connection.baseName)
  const [busy, setBusy] = useState(false)
  const [copied, setCopied] = useState(false)
  const [message, setMessage] = useState<string | null>(null)
  const [success, setSuccess] = useState(false)
  const submit = async () => {
    const plan = planConnectionEdit({ originalName: connection.baseName, name })
    if (plan.error) return setMessage(plan.error)
    setBusy(true)
    setMessage(null)
    try {
      if (plan.rename) await onRename?.(plan.rename)
      const result = await api.fetchCredentials(connection.id)
      if (!result.ok) throw new Error(result.message || "没有读取到有效凭证，请先完成终端登录")
      await onUpdated()
      setSuccess(true)
      setMessage(isFetchOnlyPlatform(connection.platformId) ? "本机来源检测通过；未更改外部账号或配置。" : "已更新本机凭证。额度查询结果以连接页面实际返回为准。")
    } catch (error) {
      setMessage(String(error))
    } finally {
      setBusy(false)
    }
  }
  if (isFetchOnlyPlatform(connection.platformId)) return (
    <Dialog title="本机来源" exiting={exiting} onClose={busy ? undefined : onClose} footer={<>
      <FooterBtn onClick={onClose} disabled={busy}>关闭</FooterBtn>
      <FooterBtn variant="primary" disabled={busy} onClick={() => void submit()}>{busy ? "正在检测…" : "保存并检测"}</FooterBtn>
    </>}>
      <Field label="连接名称" value={name} onChange={setName} placeholder="输入连接名称" />
      <p className="text-xs text-text-secondary">检测对应应用的本机数据来源；登录与切换账号请在该应用中完成。</p>
      {message && <p role={success ? "status" : "alert"} className="text-xs">{message}</p>}
    </Dialog>
  )
  return (
    <Dialog title="CLI 授权" exiting={exiting} onClose={busy ? undefined : onClose} footer={<>
      <FooterBtn onClick={onClose} disabled={busy}>{success ? "完成" : "取消"}</FooterBtn>
      {!success && <FooterBtn variant="primary" disabled={busy} onClick={() => void submit()}>{busy ? "正在读取…" : "已登录，重新获取"}</FooterBtn>}
    </>}>
      <p className="text-xs text-text-secondary">为「{connection.name}」重新授权。请在本机终端执行以下命令，再按提示完成浏览器登录。</p>
      <Field label="连接名称" value={name} onChange={setName} placeholder="输入连接名称" />
      <p className="text-[11px] leading-[1.6] text-text-tertiary">名称在点击「已登录，重新获取」时一并保存；凭证读取失败不影响已保存的名称。</p>
      <div className="flex items-center gap-2 rounded-lg border bg-surface-2 p-3">
        <code className="flex-1 select-text text-xs text-text-primary">{command}</code>
        <FooterBtn onClick={() => {
          void navigator.clipboard.writeText(command).then(() => setCopied(true))
            .catch(() => setMessage("复制失败，请选中左侧命令手动复制。"))
        }}>{copied ? "已复制" : "复制命令"}</FooterBtn>
      </div>
      <p className="text-[11px] leading-relaxed text-text-tertiary">若提示找不到命令，请先安装对应 CLI 并重新打开终端。登录时使用此连接对应的账号；取消此弹窗不会退出 CLI 或移除连接。</p>
      {message && <p role={success ? "status" : "alert"} className={cn("text-xs leading-relaxed", success ? "text-success-text" : "text-danger")}>{message}</p>}
    </Dialog>
  )
}

/*
 * 弹窗外壳（自适应，鼠鼠需求）：宽度可为像素或百分比（添加/编辑 = 主窗口 80%），
 * 永不超过窗口（maxWidth 100%）；高度上限为窗口内容区，超出时正文内部滚动，
 * 头与脚固定。圆角 r-card，头 padding [15,20]，体 20，脚 [14,20]。
 */
function Dialog({
  title,
  step,
  width = 480,
  children,
  footer,
  onClose,
  exiting = false,
}: {
  title: string
  step?: string
  /** 像素（如 480）或百分比字符串（如 "80%"，相对弹窗遮罩内容区） */
  width?: number | string
  children: React.ReactNode
  footer: React.ReactNode
  onClose?: () => void
  exiting?: boolean
}) {
  const titleId = useId()
  const dialogRef = useRef<HTMLDivElement>(null)
  const closeRef = useRef(onClose)
  closeRef.current = onClose
  useEffect(() => {
    if (exiting) return
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const dialog = dialogRef.current
    if (!dialog) return
    const focusable = () => Array.from(dialog.querySelectorAll<HTMLElement>(
      'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
    ))
    ;(focusable()[0] ?? dialog).focus()
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault()
        closeRef.current?.()
        return
      }
      if (event.key !== "Tab") return
      const items = focusable()
      if (!items.length) {
        event.preventDefault()
        return
      }
      const first = items[0]
      const last = items[items.length - 1]
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault()
        first.focus()
      }
    }
    document.addEventListener("keydown", onKeyDown)
    return () => {
      document.removeEventListener("keydown", onKeyDown)
      previous?.focus()
    }
  }, [exiting])

  return (
    <div
      ref={dialogRef}
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
      tabIndex={-1}
      className="motion-dialog flex max-h-full flex-col overflow-hidden rounded-card border bg-surface shadow-dialog"
      style={{ width, maxWidth: "100%" }}
    >
      <header className="flex w-full shrink-0 items-center justify-between border-b px-5 py-[15px]">
        <div className="flex items-center gap-2.5">
          <h3 id={titleId} className="text-sm font-semibold text-text-primary">{title}</h3>
          {step && <Chip tone="neutral" mono>{step}</Chip>}
        </div>
        <button type="button" onClick={onClose} aria-label="关闭">
          <X className="size-[15px] text-text-secondary" />
        </button>
      </header>
      <div className="flex min-h-0 w-full flex-1 flex-col gap-3 overflow-y-auto p-5">{children}</div>
      <footer className="flex w-full shrink-0 items-center justify-end gap-2 border-t bg-surface-2 px-5 py-3.5">
        {footer}
      </footer>
    </div>
  )
}

function FooterBtn({
  children,
  variant,
  disabled,
  onClick,
}: {
  children: React.ReactNode
  variant?: "primary" | "danger"
  disabled?: boolean
  onClick?: () => void
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "motion-button rounded-[8px] px-4 py-2 text-xs",
        variant === "primary" && "bg-accent-blue font-semibold text-white",
        variant === "danger" && "bg-danger font-semibold text-white",
        !variant && "border bg-surface text-text-primary",
        disabled && "pointer-events-none opacity-45",
      )}
    >
      {children}
    </button>
  )
}

function OptionRow({
  logo,
  name,
  note,
  selected,
  disabled,
  badge,
  onClick,
}: {
  logo?: PlatformId
  name: string
  note: string
  selected: boolean
  disabled?: boolean
  badge?: { label: string; tone: "success" | "accent" }
  onClick?: () => void
}) {
  return (
    <button
      type="button"
      aria-label={name}
      aria-pressed={selected}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "flex w-full items-center gap-[11px] rounded-[10px] border p-3 text-left",
        selected ? "border-[1.5px] border-accent-blue bg-accent-blue-soft" : "bg-surface",
        disabled && "pointer-events-none opacity-45",
      )}
    >
      <Radio checked={selected} />
      {logo && <PlatformLogo platform={logo} />}
      <span className="flex min-w-0 flex-1 flex-col gap-[3px]">
        <span className="flex items-center gap-[7px]">
          <span className="text-[13px] font-medium text-text-primary">{name}</span>
          {badge && <Chip tone={badge.tone} mono>{badge.label}</Chip>}
        </span>
        <span className="text-[11px] leading-[1.5] text-text-tertiary">{note}</span>
      </span>
    </button>
  )
}

/**
 * 表单输入。API Key 用 password 类型，避免明文停留在屏幕上被旁人或录屏看到；
 * trailing 可挂「小眼睛」按钮切换明文/密文（添加时看已输入内容，编辑时看回填原值）。
 */
function Field({
  label,
  value,
  onChange,
  placeholder,
  mono,
  password,
  trailing,
}: {
  label: string
  value: string
  onChange: (v: string) => void
  placeholder?: string
  mono?: boolean
  password?: boolean
  /** 输入框尾部悬浮的操作按钮（如显示/隐藏密码的小眼睛） */
  trailing?: React.ReactNode
}) {
  return (
    <label className="flex w-full flex-col gap-1.5">
      <span className="text-xs text-text-secondary">{label}</span>
      <span className="relative block w-full">
        <input
          type={password ? "password" : "text"}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
          spellCheck={false}
          autoComplete="off"
          className={cn(
            "w-full rounded-[8px] border bg-surface px-[11px] py-[9px] text-xs text-text-primary",
            "outline-none placeholder:text-text-tertiary focus:border-accent-blue",
            mono && "font-mono",
            trailing && "pr-10",
          )}
        />
        {trailing && (
          <span className="absolute inset-y-0 right-0 flex items-center pr-2.5">{trailing}</span>
        )}
      </span>
    </label>
  )
}

/**
 * 添加连接（画布 18）：平台 → 接入方式 → 完成接入。
 * 接入方式三种：官方订阅（本机 CLI）、API Key 本机读取、API Key 手动填写。
 * 手动填写支持自定义名称 / Base URL / API Key，可从网关拉取模型与思考强度，
 * 并为 Claude 连接设置 1M 上下文；结果经顶部 Toast 反馈（成功关闭、失败留在表单）。
 */
export function AddConnectionDialog({ onClose, onRead, onAddManual, exiting = false, initialPlatform = "claude", initialProvider }: {
  onClose: () => void
  onRead: (platform: string, kind: "auth" | "api", name?: string) => Promise<{ added: number; existing: number; warnings?: string[] }>
  /** 手动填写路径：凭证与默认参数经后端网络验证通过后入库 */
  onAddManual: (input: {
    platform: string
    kind: "api"
    name: string
    secret: string
    base_url: string
    model?: string | null
    effort?: string | null
    context_1m?: boolean | null
  }) => Promise<void>
  exiting?: boolean
  /** 从连接管理某个平台页打开时预选该平台，仍可在第 1 步改选 */
  initialPlatform?: PlatformId
  /** 点供应商标签打开时：跳过前两步直接进手动填写并预填端点模板 */
  initialProvider?: ProviderPreset | null
}) {
  const toast = useToast()
  const preset = initialProvider ?? null
  const [step, setStep] = useState<1 | 2 | 3>(preset ? 3 : 1)
  const [platform, setPlatform] = useState<PlatformId>(preset?.platform ?? initialPlatform)
  const [kind, setKind] = useState<"auth" | "api">(preset ? "api" : platformSupports(initialPlatform, "auth_connection") ? "auth" : "api")
  const [mode, setMode] = useState<"local" | "manual">(preset ? "manual" : "local")
  const [name, setName] = useState(preset?.name ?? "")
  // 手动填写字段
  const [baseUrl, setBaseUrl] = useState(preset?.baseUrl ?? "")
  const [secret, setSecret] = useState("")
  const [secretVisible, setSecretVisible] = useState(false)
  const [models, setModels] = useState<string[]>([])
  const [model, setModel] = useState("")
  const [effort, setEffort] = useState("medium")
  const [context1m, setContext1m] = useState(false)
  const [fetchingModels, setFetchingModels] = useState(false)
  const [busy, setBusy] = useState(false)
  const pending = useRef(false)
  const [error, setError] = useState<string | null>(null)
  const [result, setResult] = useState<{ added: number; existing: number; warnings?: string[] } | null>(null)
  const platformName = PLATFORMS.find((item) => item.id === platform)?.name ?? platform
  const officialKeyOnly = platform === "gemini" || platform === "grok"
  const officialBase = platform === "gemini" ? "https://generativelanguage.googleapis.com/v1beta/openai" : "https://api.x.ai/v1"
  const canFetch = Boolean(baseUrl.trim() && secret.trim())

  const resetManual = () => {
    setBaseUrl(""); setSecret(""); setModels([]); setModel("")
    setEffort("medium"); setContext1m(false)
  }
  const read = async () => {
    if (pending.current) return
    pending.current = true
    setBusy(true)
    setError(null)
    try {
      const summary = await onRead(platform, kind, name.trim() || undefined)
      if (!summary.added && !summary.existing) throw new Error(`未找到 ${platformName} 的${kind === "auth" ? "官方订阅" : "API Key"}配置，请先完成本机配置后重试。`)
      setResult(summary)
    } catch (reason) { setError(String(reason)) }
    finally { pending.current = false; setBusy(false) }
  }
  const submitManual = async () => {
    if (!baseUrl.trim()) return setError("请填写 Base URL")
    if (!secret.trim()) return setError("请填写 API Key")
    if (pending.current) return
    pending.current = true
    setBusy(true)
    setError(null)
    try {
      await onAddManual({
        platform,
        kind: "api",
        name: name.trim() || `${platformName} API Key`,
        secret: secret.trim(),
        base_url: baseUrl.trim(),
        model: model.trim() || null,
        effort: officialKeyOnly ? null : effort || null,
        context_1m: platform === "claude" ? context1m : null,
      })
      toast.success("连接添加成功，已通过验证")
      onClose()
    } catch (reason) {
      const message = String(reason)
      setError(message)
      toast.danger("验证失败，连接未保存", message)
    } finally { pending.current = false; setBusy(false) }
  }
  const doFetchModels = async () => {
    if (!canFetch) return setError("请先填写 Base URL 与 API Key")
    setFetchingModels(true)
    setError(null)
    try {
      const list = await api.fetchRemoteModels(platform, baseUrl.trim(), secret.trim())
      setModels(list)
      if (!list.includes(model.trim())) setModel(list[0] ?? "")
      toast.success(`已获取 ${list.length} 个模型`)
    } catch (reason) {
      toast.danger("获取模型失败", String(reason))
    } finally { setFetchingModels(false) }
  }
  const modelOptions = models.map((m) => ({ value: m, label: m }))
  // 思考强度按模型直接选择，固定四档、英文原值展示，无获取按钮
  const { effortOptions, fetchingEfforts, refreshEfforts } = useEffortChoices(model, platform, baseUrl, secret)
  // 档位列表随模型变化后，当前选中值不在支持列表则回落第一档
  useEffect(() => {
    setEffort((cur) => (effortOptions.some((o) => o.value === cur) ? cur : effortOptions[0]?.value ?? "medium"))
  }, [effortOptions])
  const submittingManual = busy && mode === "manual" && kind === "api"
  return <Dialog title="添加连接" step={`第 ${step} 步 / 共 3 步`} width={DIALOG_WIDTH} exiting={exiting} onClose={busy ? undefined : onClose}
    footer={result ? <FooterBtn variant="primary" onClick={onClose}>完成</FooterBtn> : <>
      <FooterBtn disabled={busy} onClick={step === 1 ? onClose : () => { setError(null); setStep(step === 3 ? 2 : 1) }}>{step === 1 ? "取消" : "上一步"}</FooterBtn>
      <FooterBtn variant="primary" disabled={busy} onClick={
        step === 3
          ? (kind === "api" && mode === "manual" ? () => void submitManual() : () => void read())
          : () => { setError(null); setStep(step === 1 ? 2 : 3) }
      }>{busy ? (submittingManual ? "正在验证…" : "正在读取…") : step === 3 ? (kind === "api" && mode === "manual" ? "验证并添加" : "读取并添加") : "下一步"}</FooterBtn>
    </>}>
    {step === 1 && <>
      <p className="text-xs text-text-secondary">选择要接入的平台</p>
      {PLATFORMS.map((item) => <OptionRow key={item.id} logo={item.id} name={item.name}
        note={item.limitation ?? "读取本机已配置的官方订阅或 API Key"}
        selected={platform === item.id} disabled={item.availability === "not-integrated"}
        onClick={() => { setPlatform(item.id); setKind(platformSupports(item.id, "auth_connection") ? "auth" : "api"); resetManual() }} />)}
    </>}
    {step === 2 && <>
      <p className="text-xs text-text-secondary">{platformName} · 选择接入方式</p>
      <OptionRow name={isFetchOnlyPlatform(platform) ? "本机获取" : "官方订阅"} note={isFetchOnlyPlatform(platform) ? "检测本机数据来源；启用后采集记录，不切换外部账号。" : "读取本机 CLI 已登录的 Auth 凭证。"} badge={{label:isFetchOnlyPlatform(platform) && platform !== "grok" ? "本机" : "Auth",tone:"success"}} selected={kind === "auth"} disabled={!platformSupports(platform,"auth_connection")} onClick={() => setKind("auth")} />
      <OptionRow name="API Key · 本机读取" note="读取本机已应用的 Key 和服务地址，无需填写。" badge={{label:"API",tone:"accent"}} selected={kind === "api" && mode === "local"} disabled={!platformSupports(platform,"api_connection") || isFetchOnlyPlatform(platform)} onClick={() => { setKind("api"); setMode("local") }} />
      <OptionRow name="API Key · 手动填写" note={officialKeyOnly ? "填写官方 Key，只读检测；不切换外部登录，不查询账户余额。" : "粘贴服务地址与 Key；可拉取模型列表并设置默认参数。"} badge={{label:"API",tone:"accent"}} selected={kind === "api" && mode === "manual"} disabled={!platformSupports(platform,"api_connection")} onClick={() => { setKind("api"); setMode("manual"); if (officialKeyOnly) setBaseUrl(officialBase) }} />
    </>}
    {step === 3 && kind === "api" && mode === "manual" && <>
      <div className="flex items-center gap-2 rounded-lg border bg-surface-2 p-3 text-sm"><PlatformLogo platform={platform} />{platformName}<Chip tone="accent">API Key · 手动填写</Chip></div>
      {/* 供应商快捷条（画布 18）：点一下自动填入端点模板，可再手动修改 */}
      {!officialKeyOnly && <div className="flex w-full flex-col gap-1.5">
        <span className="text-xs text-text-secondary">供应商（点击自动填入服务地址，可再修改）</span>
        <div className="flex flex-wrap gap-1.5">
          {PROVIDER_PRESETS.map((p) => {
            const selected = baseUrl === p.baseUrl && p.baseUrl !== ""
            return (
              <button
                key={p.id}
                type="button"
                aria-pressed={selected}
                onClick={() => {
                  setPlatform(p.platform)
                  setBaseUrl(p.baseUrl)
                  if (!name.trim()) setName(p.name)
                }}
                className={cn(
                  "flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11px] transition-colors",
                  selected ? "border-accent-blue bg-accent-blue-soft font-semibold text-accent-blue" : "bg-surface text-text-secondary hover:border-border-strong hover:text-text-primary",
                )}
              >
                <ProviderLogo provider={p.id} />
                {p.name}
              </button>
            )
          })}
        </div>
      </div>}
      <div className="grid w-full grid-cols-2 gap-3">
        <Field label="连接名称（选填）" value={name} onChange={setName} placeholder="留空则自动生成名称" />
        {officialKeyOnly ? <div className="flex flex-col gap-1.5 text-xs"><span>官方服务地址</span><code className="break-all text-text-secondary">{officialBase}</code></div> : <Field label="Base URL（必填）" value={baseUrl} onChange={setBaseUrl} mono placeholder="https://api.anthropic.com" />}
      </div>
      <p className="text-[11px] leading-relaxed text-text-tertiary">{officialKeyOnly ? "Key 仅保存到本机，并发送给上述官方接口检测；检测不产生模型调用，不提供订阅额度或账户余额。" : "官方直连或兼容网关地址；Key 保存在本机数据库，验证时发送给指定服务，列表只显示脱敏标识。"}</p>
      <Field
        label="API Key（必填）"
        value={secret}
        onChange={setSecret}
        password={!secretVisible}
        mono
        placeholder="粘贴 API Key"
        trailing={<EyeToggle revealed={secretVisible} onToggle={() => setSecretVisible((v) => !v)} />}
      />
      {!officialKeyOnly && <div className="grid w-full grid-cols-2 items-start gap-3">
      <div className="flex w-full flex-col gap-1.5">
        <span className="text-xs text-text-secondary">默认模型</span>
        <div className="flex w-full items-center gap-2">
          {models.length > 0 ? (
            <SelectField className="h-9 min-w-0 flex-1" label="默认模型" value={model} options={modelOptions} onValueChange={setModel} />
          ) : (
            <input
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder="获取模型后从列表选择，或直接输入模型名"
              spellCheck={false}
              autoComplete="off"
              className="h-9 min-w-0 flex-1 rounded-[8px] border bg-surface px-[11px] font-mono text-xs text-text-primary outline-none placeholder:text-text-tertiary focus:border-accent-blue"
            />
          )}
          <button
            type="button"
            disabled={fetchingModels || !canFetch}
            title={canFetch ? "从服务地址拉取可用模型列表" : "请先填写 Base URL 与 API Key"}
            onClick={() => void doFetchModels()}
            className="flex shrink-0 items-center gap-1.5 rounded-[8px] border bg-surface px-3 py-2 text-xs text-text-primary disabled:opacity-50"
          >
            {fetchingModels ? <LoaderCircle aria-hidden className="size-3.5 animate-spin" /> : <RefreshCw aria-hidden className="size-3.5" />}
            {fetchingModels ? "正在获取" : "获取模型"}
          </button>
        </div>
        <span className="text-[11px] leading-[1.5] text-text-tertiary">模型用于成本估算的价目匹配；自建网关的模型可手动输入。</span>
      </div>
      <div className="flex w-full flex-col gap-1.5">
        <span className="text-xs text-text-secondary">思考强度</span>
        <div className="flex w-full items-center gap-2">
          <SelectField className="h-9 min-w-0 flex-1" label="思考强度" value={effort} options={effortOptions} onValueChange={setEffort} />
          <button
            type="button"
            disabled={fetchingEfforts}
            title="拉取官方最新思考强度档位并存入本机，之后可在下拉中选择"
            onClick={() => void refreshEfforts()}
            className="flex shrink-0 items-center gap-1.5 rounded-[8px] border bg-surface px-3 py-2 text-xs text-text-primary disabled:opacity-50"
          >
            {fetchingEfforts ? <LoaderCircle aria-hidden className="size-3.5 animate-spin" /> : <RefreshCw aria-hidden className="size-3.5" />}
            {fetchingEfforts ? "正在获取" : "获取强度"}
          </button>
        </div>
        <span className="text-[11px] leading-[1.5] text-text-tertiary">按所选模型展示支持的档位（英文即 API 参数原值）；「获取强度」拉取官方最新对照并存入本机。</span>
      </div>
      </div>}
      {platform === "claude" && (
        <div className="flex w-full items-center justify-between gap-3">
          <div className="flex min-w-0 flex-col gap-0.5">
            <span className="text-xs text-text-secondary">1M 上下文</span>
            <span className="text-[11px] leading-[1.5] text-text-tertiary">仅 Claude 支持；开启后成本估算按 1M 档价目计算</span>
          </div>
          <Toggle on={context1m} onChange={setContext1m} label="1M 上下文" />
        </div>
      )}
      {error && <p role="alert" className="text-xs text-danger">{error}</p>}
    </>}
    {step === 3 && !(kind === "api" && mode === "manual") && <>
      <div className="flex items-center gap-2 rounded-lg border bg-surface-2 p-3 text-sm"><PlatformLogo platform={platform} />{platformName}<Chip tone={kind === "auth" ? "success" : "accent"}>{isFetchOnlyPlatform(platform) && platform !== "grok" ? "本机来源" : kind === "auth" ? "官方订阅 · Auth" : "API Key · 本机读取"}</Chip></div>
      <Field label="连接名称（选填）" value={name} onChange={setName} placeholder="留空则自动生成名称" />
      <p className="text-xs leading-relaxed text-text-secondary">{isFetchOnlyPlatform(platform) ? "请先在对应应用登录并产生会话，检测后添加本机监控。" : kind === "auth" ? "请先在对应 CLI 完成登录，再读取本机授权凭证。" : "请先在 CC Switch 中配置并应用到所选平台，再读取本机的 Key 和服务地址。"}{isFetchOnlyPlatform(platform) ? "本机监控不复制登录凭证。" : "凭证从本机读取，不需要粘贴。"}</p>
      <p className="text-[11px] leading-relaxed text-text-tertiary">仅读取所选平台与类型。已有连接会去重，不修改原始配置；添加后请在连接管理中启用。</p>
      {result && <p role="status" className="flex items-center gap-2 text-xs text-success-text"><ShieldCheck className="size-4" />{result.added ? `已添加 ${result.added} 个连接` : "该连接已存在，无需重复添加"}</p>}
      {result?.warnings?.map((warning) => <p key={warning} role="status" className="text-xs text-warn">{warning}</p>)}
      {error && <p role="alert" className="text-xs text-danger">{error}</p>}
    </>}
  </Dialog>
}

/** 小眼睛：切换 API Key 的明文/密文显示（tabIndex=-1，避免打断输入的 Tab 流） */
function EyeToggle({ revealed, onToggle }: { revealed: boolean; onToggle: () => void }) {
  return (
    <button
      type="button"
      tabIndex={-1}
      aria-label={revealed ? "隐藏 API Key" : "显示 API Key"}
      title={revealed ? "隐藏" : "显示"}
      onClick={onToggle}
      className="text-text-tertiary transition-colors hover:text-text-secondary"
    >
      {revealed ? <EyeOff aria-hidden className="size-3.5" /> : <Eye aria-hidden className="size-3.5" />}
    </button>
  )
}

/**
 * 统一编辑连接（画布 18）：与添加弹窗同宽（主面板 80%）、同表单结构，预填现有值。
 * auth 连接只可改名称（凭证由 CLI 管理）；api 连接可改名称、网关地址、API Key
 * （留空保持不变）与默认参数（模型/思考强度/1M）。结果经顶部 Toast 反馈。
 */
export function EditConnectionDialog({ connection, onClose, onSubmit, exiting = false }: {
  connection: Connection
  onClose: () => void
  onSubmit: (input: {
    id: string
    name: string
    base_url?: string | null
    secret?: string | null
    model?: string | null
    effort?: string | null
    context_1m?: boolean | null
  }) => Promise<void>
  exiting?: boolean
}) {
  const toast = useToast()
  const [name, setName] = useState(connection.baseName)
  const [baseUrl, setBaseUrl] = useState(connection.baseUrl ?? "")
  const [secret, setSecret] = useState("")
  const [secretVisible, setSecretVisible] = useState(false)
  const [models, setModels] = useState<string[]>([])
  const [model, setModel] = useState(connection.model ?? "")
  const [effort, setEffort] = useState(connection.effort ?? "medium")
  const [context1m, setContext1m] = useState(connection.context1m ?? false)
  const [fetchingModels, setFetchingModels] = useState(false)
  const [busy, setBusy] = useState(false)
  const pending = useRef(false)
  const [copied, setCopied] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const platformName = PLATFORMS.find((item) => item.id === connection.platformId)?.name ?? connection.platformId
  const officialKeyOnly = connection.platformId === "gemini" || connection.platformId === "grok"
  const isApi = connection.kind === "api"
  const isClaude = connection.platformId === "claude"

  // 全量回填（鼠鼠要求）：打开编辑即取回原 Key 填入输入框，默认密文显示、小眼睛切换明文
  useEffect(() => {
    if (!isApi) return
    api.revealConnectionSecret(connection.id)
      .then((value) => setSecret(value))
      .catch(() => {})
  }, [connection.id, isApi])
  const canFetch = Boolean(baseUrl.trim() && secret.trim())
  const doFetchModels = async () => {
    if (!baseUrl.trim()) return setError("请先填写 Base URL")
    if (!secret.trim()) return setError("请先填写 API Key")
    setFetchingModels(true)
    setError(null)
    try {
      const list = await api.fetchRemoteModels(connection.platformId, baseUrl.trim(), secret.trim())
      setModels(list)
      if (!list.includes(model.trim())) setModel(list[0] ?? "")
      toast.success(`已获取 ${list.length} 个模型`)
    } catch (reason) {
      toast.danger("获取模型失败", String(reason))
    } finally { setFetchingModels(false) }
  }
  const submit = async () => {
    if (!name.trim()) return setError("连接名称不能为空")
    if (pending.current) return
    pending.current = true
    setBusy(true)
    setError(null)
    try {
      await onSubmit({
        id: connection.id,
        name: name.trim(),
        base_url: isApi ? baseUrl.trim() : null,
        secret: secret.trim() || null,
        model: isApi ? (model.trim() || null) : null,
        effort: isApi && !officialKeyOnly ? (effort || null) : null,
        context_1m: isApi && isClaude ? context1m : null,
      })
      toast.success(`已保存 ${name.trim()}`)
      onClose()
    } catch (reason) {
      const message = String(reason)
      setError(message)
      toast.danger("保存失败", message)
    } finally { pending.current = false; setBusy(false) }
  }
  const modelOptions = models.map((m) => ({ value: m, label: m }))
  const { effortOptions, fetchingEfforts, refreshEfforts } = useEffortChoices(model, connection.platformId, baseUrl, secret)
  // 档位列表随模型变化后，当前选中值不在支持列表则回落第一档
  useEffect(() => {
    setEffort((cur) => (effortOptions.some((o) => o.value === cur) ? cur : effortOptions[0]?.value ?? "medium"))
  }, [effortOptions])
  return <Dialog title={`编辑连接 · ${platformName}`} width={DIALOG_WIDTH} exiting={exiting} onClose={busy ? undefined : onClose}
    footer={<>
      <FooterBtn disabled={busy} onClick={onClose}>取消</FooterBtn>
      <FooterBtn variant="primary" disabled={busy} onClick={() => void submit()}>{busy ? "正在保存…" : "保存"}</FooterBtn>
    </>}>
    <div className="flex items-center gap-2 rounded-lg border bg-surface-2 p-3 text-sm">
      <PlatformLogo platform={connection.platformId} />
      {platformName}
      <Chip tone={isApi ? "accent" : "success"}>{isApi ? "API Key" : "官方订阅 · Auth"}</Chip>
      {isApi && connection.masked && <span className="tnum font-mono text-[11px] text-text-tertiary">{connection.masked}</span>}
    </div>
    <div className="grid w-full grid-cols-2 gap-3">
      <Field label="连接名称" value={name} onChange={setName} placeholder="输入连接名称" />
      {isApi ? (
        officialKeyOnly ? <p className="self-end text-xs text-text-secondary">官方 Key，只读检测；不修改外部应用登录。</p> : <Field label="Base URL" value={baseUrl} onChange={setBaseUrl} mono placeholder="https://api.anthropic.com" />
      ) : (
        <div className="flex flex-col justify-end gap-1.5">
          <span className="text-xs text-text-secondary">凭证</span>
          <span className="rounded-[8px] border bg-surface-2 px-[11px] py-[9px] text-[11px] leading-[1.5] text-text-tertiary">{isFetchOnlyPlatform(connection.platformId) ? "本机来源监控；此处可修改名称，登录由对应应用管理" : "官方订阅凭证由 CLI 登录管理，此处仅可修改名称"}</span>
        </div>
      )}
    </div>
    {!isApi && !isFetchOnlyPlatform(connection.platformId) && (
      <div className="flex w-full flex-col gap-2">
        <div className="flex items-center gap-2 rounded-lg border bg-surface-2 p-3">
          <code className="flex-1 select-text text-xs text-text-primary">{connection.platformId === "claude" ? "claude auth login" : "codex login"}</code>
          <FooterBtn onClick={() => {
            const command = connection.platformId === "claude" ? "claude auth login" : "codex login"
            void navigator.clipboard.writeText(command).then(() => setCopied(true))
              .catch(() => setError("复制失败，请选中命令手动复制。"))
          }}>{copied ? "已复制" : "复制命令"}</FooterBtn>
        </div>
        <p className="text-[11px] leading-relaxed text-text-tertiary">在本机终端完成登录后，点连接行内「检测」验证可用性；改名随「保存」一并生效。</p>
      </div>
    )}
    {isApi && (
      <>
        <Field
          label="API Key"
          value={secret}
          onChange={setSecret}
          password={!secretVisible}
          mono
          placeholder="留空则保持当前 Key 不变"
          trailing={<EyeToggle revealed={secretVisible} onToggle={() => setSecretVisible((v) => !v)} />}
        />
        <p className="text-[11px] leading-relaxed text-text-tertiary">已回填当前 Key，点小眼睛可查看；修改后保存前会先验证可用性，验证失败不落库。</p>
        {!officialKeyOnly && <div className="grid w-full grid-cols-2 items-start gap-3">
          <div className="flex w-full flex-col gap-1.5">
            <span className="text-xs text-text-secondary">默认模型</span>
            <div className="flex w-full items-center gap-2">
              {models.length > 0 ? (
                <SelectField className="h-9 min-w-0 flex-1" label="默认模型" value={model} options={modelOptions} onValueChange={setModel} />
              ) : (
                <input
                  value={model}
                  onChange={(e) => setModel(e.target.value)}
                  placeholder="获取模型后从列表选择，或直接输入模型名"
                  spellCheck={false}
                  autoComplete="off"
                  className="h-9 min-w-0 flex-1 rounded-[8px] border bg-surface px-[11px] font-mono text-xs text-text-primary outline-none placeholder:text-text-tertiary focus:border-accent-blue"
                />
              )}
              <button
                type="button"
                disabled={fetchingModels || !canFetch}
                title="从服务地址拉取可用模型列表"
                onClick={() => void doFetchModels()}
                className="flex shrink-0 items-center gap-1.5 rounded-[8px] border bg-surface px-3 py-2 text-xs text-text-primary disabled:opacity-50"
              >
                {fetchingModels ? <LoaderCircle aria-hidden className="size-3.5 animate-spin" /> : <RefreshCw aria-hidden className="size-3.5" />}
                {fetchingModels ? "正在获取" : "获取模型"}
              </button>
            </div>
            <span className="text-[11px] leading-[1.5] text-text-tertiary">模型用于成本估算的价目匹配；自建网关的模型可手动输入。</span>
          </div>
          <div className="flex w-full flex-col gap-1.5">
            <span className="text-xs text-text-secondary">思考强度</span>
            <div className="flex w-full items-center gap-2">
              <SelectField className="h-9 min-w-0 flex-1" label="思考强度" value={effort} options={effortOptions} onValueChange={setEffort} />
              <button
                type="button"
                disabled={fetchingEfforts}
                title="拉取官方最新思考强度档位并存入本机，之后可在下拉中选择"
                onClick={() => void refreshEfforts()}
                className="flex shrink-0 items-center gap-1.5 rounded-[8px] border bg-surface px-3 py-2 text-xs text-text-primary disabled:opacity-50"
              >
                {fetchingEfforts ? <LoaderCircle aria-hidden className="size-3.5 animate-spin" /> : <RefreshCw aria-hidden className="size-3.5" />}
                {fetchingEfforts ? "正在获取" : "获取强度"}
              </button>
            </div>
            <span className="text-[11px] leading-[1.5] text-text-tertiary">按所选模型展示支持的档位（英文即 API 参数原值）；「获取强度」拉取官方最新对照并存入本机。</span>
          </div>
        </div>}
        {isClaude && (
          <div className="flex w-full items-center justify-between gap-3">
            <div className="flex min-w-0 flex-col gap-0.5">
              <span className="text-xs text-text-secondary">1M 上下文</span>
              <span className="text-[11px] leading-[1.5] text-text-tertiary">仅 Claude 支持；开启后成本估算按 1M 档价目计算</span>
            </div>
            <Toggle on={context1m} onChange={setContext1m} label="1M 上下文" />
          </div>
        )}
      </>
    )}
    {error && <p role="alert" className="text-xs text-danger">{error}</p>}
  </Dialog>
}

export function ReplaceApiKeyDialog({
  connection,
  onClose,
  onSubmit,
  exiting = false,
}: {
  connection: Connection
  onClose?: () => void
  /** rename 与 replaceKey 至少一个非空；调用方负责按先改名后换 Key 的顺序执行 */
  onSubmit?: (plan: { rename: string | null; replaceKey: string | null }) => Promise<void>
  exiting?: boolean
}) {
  const [name, setName] = useState(connection.baseName)
  const [secret, setSecret] = useState("")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const submit = async () => {
    const plan = planConnectionEdit({ originalName: connection.baseName, name, secret })
    if (plan.error) return setError(plan.error)
    // 什么都没改：直接关弹窗，不发空请求
    if (!plan.rename && !plan.replaceKey) return onClose?.()
    setBusy(true)
    setError(null)
    try { await onSubmit?.({ rename: plan.rename ?? null, replaceKey: plan.replaceKey ?? null }) }
    catch (reason) { setError(String(reason)) }
    finally { setBusy(false) }
  }
  return (
    <Dialog
      title={`更换 API Key · ${connection.name}`}
      width={440}
      exiting={exiting}
      onClose={onClose}
      footer={
        <>
          <FooterBtn onClick={onClose}>取消</FooterBtn>
          <FooterBtn variant="primary" disabled={busy} onClick={() => void submit()}>
            {busy ? "保存中…" : "保存"}
          </FooterBtn>
        </>
      }
    >
      <Field label="连接名称" value={name} onChange={setName} placeholder="输入连接名称" />
      {connection.masked && (
        <p className="text-[11px] leading-[1.6] text-text-tertiary">
          脱敏标识 {connection.masked} 随 Key 自动生成，不可编辑。
        </p>
      )}
      <Field label="新的 API Key（选填）" value={secret} onChange={setSecret} password mono placeholder="留空则只保存名称；输入后先验证才替换旧 Key" />
      {error && <p className="text-[11px] font-medium text-danger">{error}</p>}
      <p className="text-[11px] leading-[1.6] text-text-tertiary">
        新 Key 验证失败时保留原凭证和连接状态，已保存的名称不受影响；成功后只保存脱敏标识供界面区分。
      </p>
    </Dialog>
  )
}

/**
 * 移除连接确认 —— 破坏性操作必须二次确认（§6.3）
 * 移除只断开连接、删除凭证与配置，**不删历史统计数据**。
 */
export function RemoveConnectionDialog({
  connection,
  usedByIsland,
  onClose,
  onConfirm,
  exiting,
}: {
  connection: Connection
  /** 被移除的连接正是灵动岛当前配置时，必须提前给出橙色警示 */
  usedByIsland?: boolean
  onClose?: () => void
  onConfirm?: () => Promise<void>
  exiting?: boolean
}) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const platform = PLATFORMS.find((p) => p.id === connection.platformId)
  return (
    <Dialog
      title="移除连接？"
      exiting={exiting}
      width={460}
      onClose={onClose}
      footer={
        <>
          <FooterBtn onClick={onClose}>取消</FooterBtn>
          <FooterBtn
            variant="danger"
            disabled={busy}
            onClick={() => {
              setBusy(true)
              setError(null)
              void onConfirm?.()
                .catch((reason) => setError(String(reason)))
                .finally(() => setBusy(false))
            }}
          >
            {busy ? "移除中…" : "移除"}
          </FooterBtn>
        </>
      }
    >
      <p className="text-[13px] font-semibold text-text-primary">
        {platform?.name ?? connection.platformId} · {connection.name}（{connection.label}）
      </p>
      <p className="text-[11px] leading-[1.6] text-text-tertiary">
        仅删除本应用保存的连接与凭证副本，保留历史统计，不影响 CC Switch 或 Claude / Codex 的配置。
      </p>
      {error && <p className="text-[11px] font-medium text-danger">{error}</p>}
      {usedByIsland && (
        <div className="flex w-full items-start gap-[9px] rounded-[10px] bg-warn-soft p-3">
          <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-warn" />
          <div className="flex min-w-0 flex-1 flex-col gap-[3px]">
            <span className="text-xs font-semibold text-warn">该连接正在被灵动岛使用</span>
            <span className="text-[11px] leading-[1.5] text-warn">
              移除后灵动岛将没有可显示的连接，请在「连接管理」中启用其他连接。不会自动换成其他账号。
            </span>
          </div>
        </div>
      )}
    </Dialog>
  )
}
