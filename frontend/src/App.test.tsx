import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import App from './App'
import type {
  AppSettings,
  AppSnapshot,
  PersistedScheduleRun,
  PersistedSession,
  Topic,
} from './types'

const apiMock = vi.hoisted(() => ({
  bootstrap: vi.fn(),
  saveBootstrap: vi.fn(),
  saveSettings: vi.fn(),
  comparePriority: vi.fn(),
  generateSchedule: vi.fn(),
  simulate: vi.fn(),
  pinSchedule: vi.fn(),
  updateSession: vi.fn(),
  postponeSession: vi.fn(),
  logSession: vi.fn(),
  manualLog: vi.fn(),
}))

vi.mock('./api', () => ({ api: apiMock }))
vi.mock('@fullcalendar/react', () => ({
  default: () => <div data-testid="calendar-widget" />,
}))
vi.mock('@fullcalendar/daygrid', () => ({ default: {} }))
vi.mock('@fullcalendar/interaction', () => ({ default: {} }))
vi.mock('@fullcalendar/timegrid', () => ({ default: {} }))

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
    targetMinutes: 90,
    deadline: '2026-07-15',
    completedMinutes: 0,
    elo: 1000,
    coreWeeklySessions: 0,
    archived: false,
    activeFocusIndex: 0,
    ...patch,
  }
}

function persistedSession(patch: Partial<PersistedSession> = {}): PersistedSession {
  return {
    id: 'session-1',
    runId: 'run-1',
    topicId: 'linear',
    topicName: 'Linear Algebra',
    focusName: 'Linear Algebra',
    date: '2026-06-29',
    startMinute: 9 * 60,
    endMinute: 9 * 60 + 45,
    status: 'planned',
    locked: false,
    explanation: {
      score: 0.8,
      factors: {
        preference: 1,
        urgency: 0.5,
        remaining: 0.5,
        core: 0,
        neglect: 0,
        pace: 0,
      },
      reason: 'Test plan',
    },
    ...patch,
  }
}

function schedule(patch: Partial<PersistedScheduleRun> = {}): PersistedScheduleRun {
  return {
    id: 'run-1',
    status: 'current',
    name: null,
    startDate: '2026-06-29',
    endDate: '2026-07-15',
    pinned: false,
    issues: [],
    sessions: [persistedSession()],
    ...patch,
  }
}

function snapshot(patch: Partial<AppSnapshot> = {}): AppSnapshot {
  return {
    bootstrap: {
      settings,
      topics: [
        topic(),
        topic({
          id: 'cuda',
          name: 'CUDA C++',
          deadline: '2026-07-20',
          elo: 980,
        }),
      ],
      studyWindows: [
        {
          id: 'window',
          kind: 'recurring',
          dayOfWeek: 1,
          date: null,
          startMinute: 18 * 60,
          endMinute: 20 * 60,
          label: 'Evening',
        },
      ],
      blockedIntervals: [],
      capacityOverrides: [],
    },
    currentSchedule: null,
    previousSchedule: null,
    referenceSchedules: [],
    ...patch,
  }
}

describe('App integration', () => {
  afterEach(() => {
    vi.clearAllMocks()
  })

  it('renders the dashboard with the always-dark application frame', async () => {
    apiMock.bootstrap.mockResolvedValue(snapshot())

    render(<App />)

    expect(await screen.findByRole('heading', { name: 'Dashboard' })).toBeInTheDocument()
    expect(screen.getByRole('main')).toHaveClass('bg-slate-950', 'text-slate-100')
    expect(screen.getByText('Local planning workspace')).toHaveClass('text-slate-400')
  })

  it('disables schedule generation until setup requirements are met', async () => {
    apiMock.bootstrap.mockResolvedValue(
      snapshot({
        bootstrap: {
          ...snapshot().bootstrap,
          topics: [topic({ deadline: null })],
          studyWindows: [],
        },
      }),
    )

    render(<App />)

    expect(await screen.findByRole('heading', { name: 'Dashboard' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /^Generate$/ })).toBeDisabled()
  })

  it('saves draft data, generates a schedule, refreshes state, and opens the calendar', async () => {
    const initial = snapshot()
    const generated = snapshot({ currentSchedule: schedule() })
    apiMock.bootstrap.mockResolvedValueOnce(initial).mockResolvedValueOnce(generated)
    apiMock.saveBootstrap.mockResolvedValue(initial)
    apiMock.generateSchedule.mockResolvedValue({ preview: { sessions: [] }, saved: null })
    const user = userEvent.setup()

    render(<App />)

    await user.click(await screen.findByRole('button', { name: /^Generate$/ }))

    await waitFor(() => {
      expect(apiMock.saveBootstrap).toHaveBeenCalledWith(initial.bootstrap)
      expect(apiMock.generateSchedule).toHaveBeenCalledWith({ persist: true })
      expect(apiMock.bootstrap).toHaveBeenCalledTimes(2)
    })
    expect(await screen.findByRole('heading', { name: 'Calendar', level: 1 })).toBeInTheDocument()
    expect(screen.getByTestId('calendar-widget')).toBeInTheDocument()
  })

  it('submits priority comparisons from the pairwise priority view', async () => {
    const initial = snapshot()
    apiMock.bootstrap.mockResolvedValue(initial)
    apiMock.comparePriority.mockResolvedValue({ update: {}, topics: initial.bootstrap.topics })
    const user = userEvent.setup()

    render(<App />)

    await user.click(await screen.findByRole('button', { name: 'Priority' }))
    await user.click(screen.getByRole('button', { name: /Linear Algebra/ }))

    await waitFor(() => {
      expect(apiMock.comparePriority).toHaveBeenCalledWith('linear', 'cuda')
      expect(apiMock.bootstrap).toHaveBeenCalledTimes(2)
    })
  })
})
