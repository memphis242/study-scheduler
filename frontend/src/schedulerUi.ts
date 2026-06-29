import type { BootstrapData, SessionStatus, Topic } from './types'

export function isSetupReady(draft: BootstrapData) {
  const targetTopics = draft.topics.filter(
    (topic) => !topic.archived && topic.targetMinutes > topic.completedMinutes,
  )

  return (
    targetTopics.every((topic) => topic.deadline) &&
    draft.studyWindows.length > 0 &&
    draft.settings.defaultDailyTopicCap > 0
  )
}

export function todayString(now = new Date()) {
  return localDate(now)
}

export function localDate(date: Date) {
  const adjusted = new Date(date.getTime() - date.getTimezoneOffset() * 60_000)
  return adjusted.toISOString().slice(0, 10)
}

export function dateToMinute(date: Date) {
  return date.getHours() * 60 + date.getMinutes()
}

export function minutesToClock(minutes: number) {
  const hours = Math.floor(minutes / 60)
  const mins = minutes % 60
  return `${String(hours).padStart(2, '0')}:${String(mins).padStart(2, '0')}`
}

export function clockToMinutes(value: string) {
  const [hours, minutes] = value.split(':').map(Number)
  return hours * 60 + minutes
}

export function minutesToHoursInput(minutes: number | null) {
  if (minutes === null) return ''
  return Number((minutes / 60).toFixed(2))
}

export function hoursToMinutes(value: string) {
  return Math.round(Number(value || 0) * 60)
}

export function hoursToNullableMinutes(value: string) {
  return value === '' ? null : hoursToMinutes(value)
}

export function formatHours(minutes: number) {
  return `${(minutes / 60).toFixed(minutes % 60 === 0 ? 0 : 1)}h`
}

export function sessionColor(status: SessionStatus) {
  switch (status) {
    case 'complete':
      return '#047857'
    case 'partial':
      return '#ca8a04'
    case 'missed':
      return '#dc2626'
    case 'locked':
      return '#0f766e'
    case 'manual':
      return '#7c3aed'
    default:
      return '#2563eb'
  }
}

export function normalizePair(pair: [number, number], length: number): [number, number] {
  if (length < 2) return [0, 0]
  const first = pair[0] % length
  let second = pair[1] % length
  if (first === second) second = (second + 1) % length
  return [first, second]
}

export function nextPair(length: number, random = Math.random): [number, number] {
  if (length < 2) return [0, 0]
  const first = Math.floor(random() * length)
  let second = Math.floor(random() * length)
  if (first === second) second = (second + 1) % length
  return [first, second]
}

export function maxDeadline(topics: Pick<Topic, 'deadline'>[]) {
  return topics
    .map((topic) => topic.deadline)
    .filter((value): value is string => Boolean(value))
    .sort()
    .at(-1)
}
