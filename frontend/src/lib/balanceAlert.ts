export function balanceAlert(amount: number | null, currency: string, threshold: number | null, targetCurrency: string, stale: boolean): boolean {
  return !stale && amount !== null && Number.isFinite(amount) && threshold !== null
    && Number.isFinite(threshold) && threshold > 0 && currency.toUpperCase() === targetCurrency.toUpperCase() && amount <= threshold
}
