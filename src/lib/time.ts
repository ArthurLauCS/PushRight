/** Persist IANA names so daylight-saving rules apply to each commit's date. */
export function readTimeZone(value: string | null): string {
  if (!value) return ''
  try { return new Intl.DateTimeFormat('en', { timeZone: value }).resolvedOptions().timeZone }
  catch { return '' }
}

const formatters = new Map<string, Intl.DateTimeFormat>()

export function formatTime(sec: number, timeZone: string): string {
  let formatter = formatters.get(timeZone)
  if (!formatter) {
    formatter = new Intl.DateTimeFormat('sv-SE', {
      timeZone: timeZone || undefined, year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', hourCycle: 'h23',
    })
    formatters.set(timeZone, formatter)
  }
  return formatter.format(new Date(sec * 1000))
}

export function calendarStats(times: number[], timeZone: string) {
  let first = '', last = ''
  const counts = new Map<string, number>()
  for (const sec of times) {
    const date = formatTime(sec, timeZone).slice(0, 10)
    const month = date.slice(0, 7)
    counts.set(month, (counts.get(month) ?? 0) + 1)
    if (!first || date < first) first = date
    if (date > last) last = date
  }
  const months: { name: string; count: number }[] = []
  const index = (date: string) => Number(date.slice(0, 4)) * 12 + Number(date.slice(5, 7)) - 1
  if (first) for (let m = index(first); m <= index(last); m++) {
    const name = `${Math.floor(m / 12)}-${String(m % 12 + 1).padStart(2, '0')}`
    months.push({ name, count: counts.get(name) ?? 0 })
  }
  return { first, last, months }
}
