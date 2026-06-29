export type PriorityWeights = {
  preference: number
  urgency: number
  remaining: number
  core: number
  neglect: number
  pace: number
}

export type AppSettings = {
  timezone: string
  granularityMinutes: number
  weekStart: string
  defaultDailyTopicCap: number
  defaultDailyCapMinutes: number | null
  defaultWeeklyCapMinutes: number | null
  defaultMonthlyCapMinutes: number | null
  planningHorizonMode: string
  priorityWeights: PriorityWeights
}

export type Topic = {
  id: string
  name: string
  members: string[]
  minSessionMinutes: number
  targetMinutes: number
  deadline: string | null
  completedMinutes: number
  elo: number
  coreWeeklySessions: number
  archived: boolean
  activeFocusIndex: number
}

export type WindowKind = 'recurring' | 'one_off'

export type AvailabilityWindow = {
  id: string
  kind: WindowKind
  dayOfWeek: number | null
  date: string | null
  startMinute: number
  endMinute: number
  label: string
}

export type CapacityOverride = {
  id: string
  date: string
  dailyCapMinutes: number | null
  topicCap: number | null
}

export type BootstrapData = {
  settings: AppSettings
  topics: Topic[]
  studyWindows: AvailabilityWindow[]
  blockedIntervals: AvailabilityWindow[]
  capacityOverrides: CapacityOverride[]
}

export type IssueSeverity = 'blocker' | 'warning'

export type FeasibilityIssue = {
  severity: IssueSeverity
  code: string
  message: string
  date: string | null
  topicId: string | null
}

export type ScoreBreakdown = {
  preference: number
  urgency: number
  remaining: number
  core: number
  neglect: number
  pace: number
}

export type SessionExplanation = {
  score: number
  factors: ScoreBreakdown
  reason: string
}

export type SessionStatus =
  | 'planned'
  | 'locked'
  | 'complete'
  | 'partial'
  | 'missed'
  | 'manual'

export type PersistedSession = {
  id: string
  runId: string
  topicId: string
  topicName: string
  focusName: string
  date: string
  startMinute: number
  endMinute: number
  status: SessionStatus
  locked: boolean
  explanation: SessionExplanation
}

export type ScheduledSession = Omit<PersistedSession, 'runId' | 'status'> & {
  topicName: string
}

export type ScheduleRunStatus =
  | 'current'
  | 'previous'
  | 'reference'
  | 'simulation'

export type PersistedScheduleRun = {
  id: string
  status: ScheduleRunStatus
  name: string | null
  startDate: string
  endDate: string
  pinned: boolean
  issues: FeasibilityIssue[]
  sessions: PersistedSession[]
}

export type SchedulePreview = {
  canGenerate: boolean
  startDate: string
  endDate: string
  sessions: ScheduledSession[]
  issues: FeasibilityIssue[]
}

export type AppSnapshot = {
  bootstrap: BootstrapData
  currentSchedule: PersistedScheduleRun | null
  previousSchedule: PersistedScheduleRun | null
  referenceSchedules: PersistedScheduleRun[]
}

export type GenerateScheduleResponse = {
  preview: SchedulePreview
  saved: PersistedScheduleRun | null
}

export type PriorityComparisonResponse = {
  update: {
    winnerBefore: number
    loserBefore: number
    winnerAfter: number
    loserAfter: number
    kFactor: number
  }
  topics: Topic[]
}

export type ScheduleInput = {
  startDate: string
  endDate: string
  settings: AppSettings
  topics: Topic[]
  studyWindows: AvailabilityWindow[]
  blockedIntervals: AvailabilityWindow[]
  capacityOverrides: CapacityOverride[]
  lastStudiedDates: Record<string, string>
}
