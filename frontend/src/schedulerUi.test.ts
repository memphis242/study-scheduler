import { describe, expect, it } from 'vitest'

import {
  clockToMinutes,
  dateToMinute,
  formatHours,
  hoursToMinutes,
  hoursToNullableMinutes,
  isSetupReady,
  localDate,
  maxDeadline,
  minutesToClock,
  minutesToHoursInput,
  nextPair,
  normalizePair,
  sessionColor,
  todayString,
} from './schedulerUi'
import type { AppSettings, AvailabilityWindow, BootstrapData, Topic } from './types'

const settings: AppSettings = {
  timezone: 'America/Chicago',
  granularityMinutes: 15,
  weekStart: 'monday',
  defaultDailyTopicCap: 2,
  defaultDailyCapMinutes: null,
  defaultWeeklyCapMinutes: null,
  defaultMonthlyCapMinutes: null,
  planningHorizonMode: 'until_deadlines',
  priorityWeights: {
    preference: 0.25,
    urgency: 0.25,
    remaining: 0.2,
    core: 0.15,
    neglect: 0.1,
    pace: 0.05,
  },
}

function topic(patch: Partial<Topic> = {}): Topic {
  return {
    id: 'linear',
    name: 'Linear Algebra',
    members: [],
    minSessionMinutes: 45,
    targetMinutes: 120,
    deadline: '2026-07-15',
    completedMinutes: 0,
    elo: 1000,
    coreWeeklySessions: 0,
    archived: false,
    activeFocusIndex: 0,
    ...patch,
  }
}

function studyWindow(patch: Partial<AvailabilityWindow> = {}): AvailabilityWindow {
  return {
    id: 'window',
    kind: 'recurring',
    dayOfWeek: 1,
    date: null,
    startMinute: 18 * 60,
    endMinute: 20 * 60,
    label: 'Evening',
    ...patch,
  }
}

function bootstrap(patch: Partial<BootstrapData> = {}): BootstrapData {
  return {
    settings,
    topics: [topic()],
    studyWindows: [studyWindow()],
    blockedIntervals: [],
    capacityOverrides: [],
    ...patch,
  }
}

describe('scheduler UI helpers', () => {
  it('converts local dates and minutes consistently', () => {
    const date = new Date(2026, 5, 29, 13, 45)

    expect(localDate(date)).toBe('2026-06-29')
    expect(todayString(date)).toBe('2026-06-29')
    expect(dateToMinute(date)).toBe(13 * 60 + 45)
    expect(minutesToClock(8 * 60 + 5)).toBe('08:05')
    expect(clockToMinutes('23:30')).toBe(23 * 60 + 30)
  })

  it('formats hour inputs and display labels', () => {
    expect(minutesToHoursInput(null)).toBe('')
    expect(minutesToHoursInput(75)).toBe(1.25)
    expect(hoursToMinutes('1.25')).toBe(75)
    expect(hoursToNullableMinutes('')).toBeNull()
    expect(hoursToNullableMinutes('2')).toBe(120)
    expect(formatHours(120)).toBe('2h')
    expect(formatHours(75)).toBe('1.3h')
  })

  it('requires deadlines, study windows, and a positive topic cap before generation', () => {
    expect(isSetupReady(bootstrap())).toBe(true)
    expect(isSetupReady(bootstrap({ topics: [topic({ deadline: null })] }))).toBe(false)
    expect(isSetupReady(bootstrap({ studyWindows: [] }))).toBe(false)
    expect(
      isSetupReady(
        bootstrap({
          settings: { ...settings, defaultDailyTopicCap: 0 },
        }),
      ),
    ).toBe(false)
  })

  it('ignores archived or already-completed target topics when checking readiness', () => {
    expect(
      isSetupReady(
        bootstrap({
          topics: [
            topic({ id: 'done', targetMinutes: 60, completedMinutes: 60, deadline: null }),
            topic({ id: 'archived', archived: true, deadline: null }),
          ],
        }),
      ),
    ).toBe(true)
  })

  it('selects the latest non-empty deadline string', () => {
    expect(
      maxDeadline([
        topic({ deadline: '2026-07-15' }),
        topic({ deadline: null }),
        topic({ deadline: '2026-08-01' }),
      ]),
    ).toBe('2026-08-01')
    expect(maxDeadline([topic({ deadline: null })])).toBeUndefined()
  })

  it('normalizes and generates valid priority pairs', () => {
    expect(normalizePair([2, 2], 4)).toEqual([2, 3])
    expect(normalizePair([5, 1], 4)).toEqual([1, 2])
    expect(normalizePair([0, 1], 1)).toEqual([0, 0])
    expect(nextPair(4, () => 0.1)).toEqual([0, 1])
    expect(nextPair(4, () => 0.9)).toEqual([3, 0])
  })

  it('maps session statuses to calendar colors', () => {
    expect(sessionColor('complete')).toBe('#047857')
    expect(sessionColor('partial')).toBe('#ca8a04')
    expect(sessionColor('missed')).toBe('#dc2626')
    expect(sessionColor('locked')).toBe('#0f766e')
    expect(sessionColor('manual')).toBe('#7c3aed')
    expect(sessionColor('planned')).toBe('#2563eb')
  })
})
