export function resolveDarkTheme(theme: string, systemDark: boolean): boolean {
  return theme === "dark" || (theme === "system" && systemDark)
}

export function refreshDelay(minutes: number | undefined): number {
  return (minutes != null && [1, 5, 15, 30].includes(minutes) ? minutes : 5) * 60_000
}
