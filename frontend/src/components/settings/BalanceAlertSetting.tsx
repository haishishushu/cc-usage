import { SettingIcon } from "./SettingIcon"
import { useEffect, useState } from "react"
import { api, type AppSettings } from "@/lib/api"
import { Button } from "@/components/ui/primitives"
import { useToast } from "@/components/ui/Toast"

export function BalanceAlertSetting({ settings }: { settings: AppSettings }) {
  const [amount, setAmount] = useState("")
  const [currency, setCurrency] = useState("USD")
  const [busy, setBusy] = useState(false)
  /** 只留表单校验提示：它要紧贴输入框，指的是「这两个框里填错了」 */
  const [message, setMessage] = useState<string | null>(null)
  const toast = useToast()
  useEffect(() => {
    setAmount(settings.balance_alert_threshold == null ? "" : String(settings.balance_alert_threshold))
    setCurrency(settings.balance_alert_currency)
  }, [settings.balance_alert_threshold, settings.balance_alert_currency])
  const save = async (disable = false) => {
    const value = disable ? null : Number(amount)
    if ((!disable && (!amount.trim() || !Number.isFinite(value) || value! <= 0 || value! > 1e9)) || !/^[A-Z]{3}$/.test(currency)) {
      setMessage("请输入三位大写币种代码和大于 0、不超过 10 亿的阈值")
      return
    }
    setBusy(true)
    setMessage(null)
    try {
      await api.setBalanceAlert(value, currency)
      if (disable) toast.success("已关闭余额提醒")
      else toast.success("已保存余额提醒", `余额低于 ${value} ${currency} 时在卡片中提醒`)
    }
    catch (error) { toast.danger("余额提醒保存失败", String(error)) }
    finally { setBusy(false) }
  }
  return <div className="flex flex-wrap items-center gap-4 rounded-[10px] border bg-surface px-3.5 py-3">
    <SettingIcon label="余额提醒" />
    <div className="flex min-w-[200px] flex-1 flex-col gap-[3px]">
      <span className="text-xs font-medium text-text-primary">余额提醒</span>
      <p className="text-[11px] leading-[1.4] text-text-tertiary">余额低于设定金额时在卡片中提醒，币种需与连接一致。</p>
    </div>
    <div className="flex flex-wrap items-center gap-2">
      <input aria-label="余额提醒币种" value={currency} maxLength={3} disabled={busy} onChange={(e) => setCurrency(e.target.value.toUpperCase())} className="h-8 w-20 rounded-lg border bg-surface px-2 text-xs text-text-primary" />
      <input aria-label="余额提醒阈值" inputMode="decimal" placeholder="提醒阈值" value={amount} disabled={busy} onChange={(e) => setAmount(e.target.value)} className="h-8 w-32 rounded-lg border bg-surface px-2 text-xs text-text-primary" />
      <Button disabled={busy} onClick={() => void save()}>保存</Button>
      <Button disabled={busy || settings.balance_alert_threshold == null} onClick={() => void save(true)}>关闭提醒</Button>
    </div>
    {message && <p role="alert" className="w-full text-xs text-danger">{message}</p>}
  </div>
}
