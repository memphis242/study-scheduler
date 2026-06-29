import { useEffect, useMemo, useState } from 'react'
import FullCalendar from '@fullcalendar/react'
import dayGridPlugin from '@fullcalendar/daygrid'
import interactionPlugin from '@fullcalendar/interaction'
import timeGridPlugin from '@fullcalendar/timegrid'
import {
  AlertTriangle,
  BarChart3,
  CalendarDays,
  CheckCircle2,
  Clock,
  GitCompareArrows,
  GripVertical,
  ListChecks,
  Play,
  Plus,
  Save,
  Settings2,
  SlidersHorizontal,
  Trash2,
} from 'lucide-react'

import { api } from './api'
import type {
  AppSnapshot,
  AvailabilityWindow,
  BootstrapData,
  FeasibilityIssue,
  PersistedSession,
  SchedulePreview,
  SessionStatus,
  Topic,
} from './types'

type ViewKey =
  | 'setup'
  | 'dashboard'
  | 'calendar'
  | 'topics'
  | 'priority'
  | 'planner'
  | 'tracking'
  | 'availability'

const views: Array<{
  key: ViewKey
  label: string
  icon: typeof CalendarDays
}> = [
  { key: 'dashboard', label: 'Dashboard', icon: BarChart3 },
  { key: 'setup', label: 'Setup', icon: Settings2 },
  { key: 'calendar', label: 'Calendar', icon: CalendarDays },
  { key: 'topics', label: 'Topics', icon: ListChecks },
  { key: 'priority', label: 'Priority', icon: GitCompareArrows },
  { key: 'planner', label: 'Planner', icon: SlidersHorizontal },
  { key: 'tracking', label: 'Tracking', icon: CheckCircle2 },
  { key: 'availability', label: 'Availability', icon: Clock },
]

const dayNames = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']
const factorNames = ['preference', 'urgency', 'remaining', 'core', 'neglect', 'pace'] as const

function App() {
  const [view, setView] = useState<ViewKey>('dashboard')
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null)
  const [draft, setDraft] = useState<BootstrapData | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [plannerPreview, setPlannerPreview] = useState<SchedulePreview | null>(null)
  const [priorityPair, setPriorityPair] = useState<[number, number]>([0, 1])

  useEffect(() => {
    refresh()
  }, [])

  useEffect(() => {
    if (snapshot) {
      setDraft(structuredClone(snapshot.bootstrap))
    }
  }, [snapshot])

  async function refresh() {
    setLoading(true)
    setError(null)
    try {
      const next = await api.bootstrap()
      setSnapshot(next)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load app state')
    } finally {
      setLoading(false)
    }
  }

  async function saveDraft() {
    if (!draft) return null
    setBusy(true)
    setError(null)
    try {
      const next = await api.saveBootstrap(draft)
      setSnapshot(next)
      return next
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save setup')
      return null
    } finally {
      setBusy(false)
    }
  }

  async function generateSchedule() {
    setBusy(true)
    setError(null)
    try {
      if (draft) {
        await api.saveBootstrap(draft)
      }
      await api.generateSchedule({ persist: true })
      await refresh()
      setView('calendar')
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to generate schedule')
    } finally {
      setBusy(false)
    }
  }

  async function choosePriority(winner: Topic, loser: Topic) {
    setBusy(true)
    setError(null)
    try {
      await api.comparePriority(winner.id, loser.id)
      const next = await api.bootstrap()
      setSnapshot(next)
      const active = next.bootstrap.topics.filter((topic) => !topic.archived)
      if (active.length > 1) {
        setPriorityPair([
          Math.floor(Math.random() * active.length),
          Math.floor(Math.random() * active.length),
        ])
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to update priority')
    } finally {
      setBusy(false)
    }
  }

  const activeTopics = useMemo(
    () => (draft?.topics ?? []).filter((topic) => !topic.archived),
    [draft],
  )

  const setupReady = useMemo(() => {
    if (!draft) return false
    const targetTopics = draft.topics.filter(
      (topic) => !topic.archived && topic.targetMinutes > topic.completedMinutes,
    )
    return (
      targetTopics.every((topic) => topic.deadline) &&
      draft.studyWindows.length > 0 &&
      draft.settings.defaultDailyTopicCap > 0
    )
  }, [draft])

  if (loading && !snapshot) {
    return (
      <main className="min-h-screen bg-slate-50 text-slate-950">
        <div className="mx-auto flex min-h-screen max-w-6xl items-center justify-center px-6">
          <div className="text-sm font-medium text-slate-600">Loading scheduler...</div>
        </div>
      </main>
    )
  }

  return (
    <main className="min-h-screen bg-slate-50 text-slate-950">
      <div className="mx-auto flex min-h-screen max-w-[1440px]">
        <aside className="hidden w-64 shrink-0 border-r border-slate-200 bg-white px-4 py-5 lg:block">
          <div className="px-2">
            <div className="text-lg font-semibold">Study Scheduler</div>
            <div className="mt-1 text-xs text-slate-500">Local planning workspace</div>
          </div>
          <nav className="mt-6 grid gap-1">
            {views.map((item) => (
              <button
                key={item.key}
                type="button"
                className={`nav-button ${view === item.key ? 'nav-button-active' : ''}`}
                onClick={() => setView(item.key)}
              >
                <item.icon size={17} />
                {item.label}
              </button>
            ))}
          </nav>
        </aside>

        <section className="min-w-0 flex-1">
          <header className="sticky top-0 z-10 border-b border-slate-200 bg-white/95 px-4 py-3 backdrop-blur lg:px-8">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <h1 className="text-xl font-semibold">{titleForView(view)}</h1>
                <p className="text-sm text-slate-500">
                  {snapshot?.currentSchedule
                    ? `Current plan: ${snapshot.currentSchedule.startDate} to ${snapshot.currentSchedule.endDate}`
                    : 'No generated plan yet'}
                </p>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <select
                  className="field lg:hidden"
                  value={view}
                  onChange={(event) => setView(event.target.value as ViewKey)}
                >
                  {views.map((item) => (
                    <option key={item.key} value={item.key}>
                      {item.label}
                    </option>
                  ))}
                </select>
                <button type="button" className="secondary-button" onClick={saveDraft} disabled={busy || !draft}>
                  <Save size={16} />
                  Save
                </button>
                <button type="button" className="primary-button" onClick={generateSchedule} disabled={busy || !setupReady}>
                  <Play size={16} />
                  Generate
                </button>
              </div>
            </div>
            {error ? <div className="mt-3 rounded border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">{error}</div> : null}
          </header>

          <div className="px-4 py-5 lg:px-8">
            {!draft || !snapshot ? null : (
              <>
                {view === 'dashboard' ? (
                  <Dashboard
                    snapshot={snapshot}
                    draft={draft}
                    setupReady={setupReady}
                    onGenerate={generateSchedule}
                    onPin={async () => {
                      if (!snapshot.currentSchedule) return
                      setSnapshot(await api.pinSchedule(snapshot.currentSchedule.id, 'Reference schedule'))
                    }}
                  />
                ) : null}
                {view === 'setup' ? (
                  <SetupView draft={draft} setDraft={setDraft} setupReady={setupReady} />
                ) : null}
                {view === 'calendar' ? (
                  <CalendarView snapshot={snapshot} refresh={refresh} />
                ) : null}
                {view === 'topics' ? (
                  <TopicsView draft={draft} setDraft={setDraft} />
                ) : null}
                {view === 'priority' ? (
                  <PriorityView
                    topics={activeTopics}
                    pair={priorityPair}
                    setPair={setPriorityPair}
                    onChoose={choosePriority}
                  />
                ) : null}
                {view === 'planner' ? (
                  <PlannerView
                    draft={draft}
                    setDraft={setDraft}
                    preview={plannerPreview}
                    setPreview={setPlannerPreview}
                  />
                ) : null}
                {view === 'tracking' ? (
                  <TrackingView snapshot={snapshot} refresh={refresh} />
                ) : null}
                {view === 'availability' ? (
                  <AvailabilityView draft={draft} setDraft={setDraft} />
                ) : null}
              </>
            )}
          </div>
        </section>
      </div>
    </main>
  )
}

function Dashboard({
  snapshot,
  draft,
  setupReady,
  onGenerate,
  onPin,
}: {
  snapshot: AppSnapshot
  draft: BootstrapData
  setupReady: boolean
  onGenerate: () => void
  onPin: () => void
}) {
  const today = todayString()
  const todaySessions = snapshot.currentSchedule?.sessions.filter((session) => session.date === today) ?? []
  const issues = snapshot.currentSchedule?.issues ?? []
  const totalRemaining = draft.topics.reduce(
    (sum, topic) => sum + Math.max(0, topic.targetMinutes - topic.completedMinutes),
    0,
  )

  return (
    <div className="grid gap-5 xl:grid-cols-[1.2fr_0.8fr]">
      <section className="panel">
        <div className="section-header">
          <div>
            <h2>Today</h2>
            <p>{todaySessions.length} planned sessions</p>
          </div>
          <button type="button" className="primary-button" disabled={!setupReady} onClick={onGenerate}>
            <Play size={16} />
            Generate Plan
          </button>
        </div>
        <div className="mt-4 grid gap-3">
          {todaySessions.length === 0 ? (
            <EmptyState text="No sessions scheduled for today." />
          ) : (
            todaySessions.map((session) => <SessionRow key={session.id} session={session} />)
          )}
        </div>
      </section>

      <section className="panel">
        <div className="section-header">
          <div>
            <h2>Feasibility</h2>
            <p>{setupReady ? 'Setup is sufficient for scheduling' : 'Setup needs deadlines and windows'}</p>
          </div>
          <StatusPill ok={setupReady && !issues.some((issue) => issue.severity === 'blocker')} />
        </div>
        <IssueList issues={issues} />
        <div className="mt-4 grid grid-cols-3 gap-3 text-sm">
          <Metric label="Topics" value={draft.topics.filter((topic) => !topic.archived).length.toString()} />
          <Metric label="Windows" value={draft.studyWindows.length.toString()} />
          <Metric label="Remaining" value={formatHours(totalRemaining)} />
        </div>
      </section>

      <section className="panel xl:col-span-2">
        <div className="section-header">
          <div>
            <h2>Progress</h2>
            <p>Target hours by topic</p>
          </div>
          <button type="button" className="secondary-button" disabled={!snapshot.currentSchedule} onClick={onPin}>
            <GripVertical size={16} />
            Pin Reference
          </button>
        </div>
        <div className="mt-4 grid gap-3 md:grid-cols-2">
          {draft.topics
            .filter((topic) => !topic.archived)
            .map((topic) => (
              <ProgressTopic key={topic.id} topic={topic} />
            ))}
        </div>
      </section>
    </div>
  )
}

function SetupView({
  draft,
  setDraft,
  setupReady,
}: {
  draft: BootstrapData
  setDraft: (next: BootstrapData) => void
  setupReady: boolean
}) {
  return (
    <div className="grid gap-5 xl:grid-cols-[0.85fr_1.15fr]">
      <section className="panel">
        <div className="section-header">
          <div>
            <h2>Plan Requirements</h2>
            <p>Minimum inputs needed before generation</p>
          </div>
          <StatusPill ok={setupReady} />
        </div>
        <div className="mt-4 grid gap-3">
          <ChecklistItem
            ok={draft.topics.some((topic) => !topic.archived && topic.targetMinutes > 0)}
            text="At least one active target-hour topic"
          />
          <ChecklistItem
            ok={draft.topics
              .filter((topic) => !topic.archived && topic.targetMinutes > topic.completedMinutes)
              .every((topic) => topic.deadline)}
            text="Every active target topic has a deadline"
          />
          <ChecklistItem ok={draft.studyWindows.length > 0} text="At least one study window exists" />
          <ChecklistItem ok={draft.settings.defaultDailyTopicCap > 0} text="Daily topic cap is greater than zero" />
        </div>
      </section>

      <section className="panel">
        <div className="section-header">
          <div>
            <h2>Defaults</h2>
            <p>Capacity, week policy, and priority weights</p>
          </div>
        </div>
        <div className="mt-4 grid gap-4 md:grid-cols-2">
          <label className="label">
            Timezone
            <input
              className="field"
              value={draft.settings.timezone}
              onChange={(event) =>
                setDraft({
                  ...draft,
                  settings: { ...draft.settings, timezone: event.target.value },
                })
              }
            />
          </label>
          <label className="label">
            Daily topic cap
            <input
              className="field"
              type="number"
              min={0}
              value={draft.settings.defaultDailyTopicCap}
              onChange={(event) =>
                setDraft({
                  ...draft,
                  settings: {
                    ...draft.settings,
                    defaultDailyTopicCap: Number(event.target.value),
                  },
                })
              }
            />
          </label>
          <label className="label">
            Daily cap hours
            <input
              className="field"
              type="number"
              min={0}
              step={0.25}
              value={minutesToHoursInput(draft.settings.defaultDailyCapMinutes)}
              onChange={(event) =>
                setDraft({
                  ...draft,
                  settings: {
                    ...draft.settings,
                    defaultDailyCapMinutes: hoursToNullableMinutes(event.target.value),
                  },
                })
              }
            />
          </label>
          <label className="label">
            Time granularity
            <input className="field" value={`${draft.settings.granularityMinutes} minutes`} disabled />
          </label>
        </div>
        <WeightEditor draft={draft} setDraft={setDraft} />
      </section>
    </div>
  )
}

function TopicsView({
  draft,
  setDraft,
}: {
  draft: BootstrapData
  setDraft: (next: BootstrapData) => void
}) {
  function updateTopic(topicId: string, patch: Partial<Topic>) {
    setDraft({
      ...draft,
      topics: draft.topics.map((topic) => (topic.id === topicId ? { ...topic, ...patch } : topic)),
    })
  }

  function addTopic() {
    setDraft({
      ...draft,
      topics: [
        ...draft.topics,
        {
          id: crypto.randomUUID(),
          name: 'New Topic',
          members: [],
          minSessionMinutes: 45,
          targetMinutes: 0,
          deadline: null,
          completedMinutes: 0,
          elo: 1000,
          coreWeeklySessions: 0,
          archived: false,
          activeFocusIndex: 0,
        },
      ],
    })
  }

  return (
    <section className="panel">
      <div className="section-header">
        <div>
          <h2>Topics</h2>
          <p>Edit targets, deadlines, session minimums, tuple members, and core session rules</p>
        </div>
        <button type="button" className="primary-button" onClick={addTopic}>
          <Plus size={16} />
          Topic
        </button>
      </div>
      <div className="mt-4 overflow-x-auto">
        <table className="data-table min-w-[1100px]">
          <thead>
            <tr>
              <th>Topic</th>
              <th>Tuple Members</th>
              <th>Min</th>
              <th>Target</th>
              <th>Completed</th>
              <th>Deadline</th>
              <th>Core</th>
              <th>Elo</th>
              <th>Archive</th>
            </tr>
          </thead>
          <tbody>
            {draft.topics.map((topic) => (
              <tr key={topic.id} className={topic.archived ? 'opacity-50' : ''}>
                <td>
                  <input
                    className="table-field min-w-48"
                    value={topic.name}
                    onChange={(event) => updateTopic(topic.id, { name: event.target.value })}
                  />
                </td>
                <td>
                  <input
                    className="table-field min-w-64"
                    value={topic.members.join(', ')}
                    onChange={(event) =>
                      updateTopic(topic.id, {
                        members: event.target.value
                          .split(',')
                          .map((member) => member.trim())
                          .filter(Boolean),
                      })
                    }
                  />
                </td>
                <td>
                  <input
                    className="table-field w-20"
                    type="number"
                    min={15}
                    step={15}
                    value={topic.minSessionMinutes}
                    onChange={(event) =>
                      updateTopic(topic.id, { minSessionMinutes: Number(event.target.value) })
                    }
                  />
                </td>
                <td>
                  <input
                    className="table-field w-24"
                    type="number"
                    min={0}
                    step={0.25}
                    value={minutesToHoursInput(topic.targetMinutes)}
                    onChange={(event) =>
                      updateTopic(topic.id, { targetMinutes: hoursToMinutes(event.target.value) })
                    }
                  />
                </td>
                <td>
                  <input
                    className="table-field w-24"
                    type="number"
                    min={0}
                    step={0.25}
                    value={minutesToHoursInput(topic.completedMinutes)}
                    onChange={(event) =>
                      updateTopic(topic.id, { completedMinutes: hoursToMinutes(event.target.value) })
                    }
                  />
                </td>
                <td>
                  <input
                    className="table-field"
                    type="date"
                    value={topic.deadline ?? ''}
                    onChange={(event) => updateTopic(topic.id, { deadline: event.target.value || null })}
                  />
                </td>
                <td>
                  <input
                    className="table-field w-20"
                    type="number"
                    min={0}
                    value={topic.coreWeeklySessions}
                    onChange={(event) =>
                      updateTopic(topic.id, { coreWeeklySessions: Number(event.target.value) })
                    }
                  />
                </td>
                <td>{Math.round(topic.elo)}</td>
                <td>
                  <input
                    type="checkbox"
                    checked={topic.archived}
                    onChange={(event) => updateTopic(topic.id, { archived: event.target.checked })}
                  />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  )
}

function AvailabilityView({
  draft,
  setDraft,
}: {
  draft: BootstrapData
  setDraft: (next: BootstrapData) => void
}) {
  return (
    <div className="grid gap-5 xl:grid-cols-2">
      <WindowEditor
        title="Study Windows"
        description="Time intervals where study may be scheduled"
        windows={draft.studyWindows}
        onChange={(studyWindows) => setDraft({ ...draft, studyWindows })}
        defaultLabel="Study"
      />
      <WindowEditor
        title="Blocked Intervals"
        description="Work, meals, appointments, commute, gym, social time, or other unavailable intervals"
        windows={draft.blockedIntervals}
        onChange={(blockedIntervals) => setDraft({ ...draft, blockedIntervals })}
        defaultLabel="Blocked"
      />
    </div>
  )
}

function CalendarView({
  snapshot,
  refresh,
}: {
  snapshot: AppSnapshot
  refresh: () => Promise<void>
}) {
  const events = useMemo(() => {
    const sessionEvents =
      snapshot.currentSchedule?.sessions.map((session) => ({
        id: session.id,
        title: `${session.focusName} · ${session.topicName}`,
        start: `${session.date}T${minutesToClock(session.startMinute)}:00`,
        end: `${session.date}T${minutesToClock(session.endMinute)}:00`,
        backgroundColor: sessionColor(session.status),
        borderColor: session.locked ? '#0f766e' : sessionColor(session.status),
        extendedProps: { session },
      })) ?? []

    const blockEvents = snapshot.bootstrap.blockedIntervals.map((block) =>
      block.kind === 'recurring'
        ? {
            id: block.id,
            title: block.label,
            daysOfWeek: [block.dayOfWeek === 7 ? 0 : block.dayOfWeek],
            startTime: minutesToClock(block.startMinute),
            endTime: minutesToClock(block.endMinute),
            display: 'background',
            backgroundColor: '#fecaca',
            editable: false,
          }
        : {
            id: block.id,
            title: block.label,
            start: `${block.date}T${minutesToClock(block.startMinute)}:00`,
            end: `${block.date}T${minutesToClock(block.endMinute)}:00`,
            display: 'background',
            backgroundColor: '#fecaca',
            editable: false,
          },
    )
    return [...sessionEvents, ...blockEvents]
  }, [snapshot])

  return (
    <section className="panel">
      <div className="section-header">
        <div>
          <h2>Calendar</h2>
          <p>Drag or resize study sessions to create locked manual overrides</p>
        </div>
      </div>
      <div className="calendar-shell mt-4">
        <FullCalendar
          plugins={[dayGridPlugin, timeGridPlugin, interactionPlugin]}
          initialView="timeGridWeek"
          headerToolbar={{
            left: 'prev,next today',
            center: 'title',
            right: 'dayGridMonth,timeGridWeek,timeGridDay',
          }}
          editable
          events={events}
          eventDrop={async (info) => {
            const session = info.event.extendedProps.session as PersistedSession | undefined
            if (!session || !info.event.start || !info.event.end) {
              info.revert()
              return
            }
            try {
              await api.updateSession(session.id, {
                date: localDate(info.event.start),
                startMinute: dateToMinute(info.event.start),
                endMinute: dateToMinute(info.event.end),
                locked: true,
                status: 'locked',
              })
              await refresh()
            } catch {
              info.revert()
            }
          }}
          eventResize={async (info) => {
            const session = info.event.extendedProps.session as PersistedSession | undefined
            if (!session || !info.event.start || !info.event.end) {
              info.revert()
              return
            }
            try {
              await api.updateSession(session.id, {
                date: localDate(info.event.start),
                startMinute: dateToMinute(info.event.start),
                endMinute: dateToMinute(info.event.end),
                locked: true,
                status: 'locked',
              })
              await refresh()
            } catch {
              info.revert()
            }
          }}
          height="auto"
          slotMinTime="05:00:00"
          slotMaxTime="24:00:00"
          nowIndicator
        />
      </div>
    </section>
  )
}

function PriorityView({
  topics,
  pair,
  setPair,
  onChoose,
}: {
  topics: Topic[]
  pair: [number, number]
  setPair: (pair: [number, number]) => void
  onChoose: (winner: Topic, loser: Topic) => void
}) {
  const normalizedPair = normalizePair(pair, topics.length)
  const first = topics[normalizedPair[0]]
  const second = topics[normalizedPair[1]]

  if (!first || !second) {
    return <EmptyState text="Add at least two active topics to run priority comparisons." />
  }

  return (
    <div className="grid gap-5 xl:grid-cols-[1fr_0.8fr]">
      <section className="panel">
        <div className="section-header">
          <div>
            <h2>Pairwise Priority Test</h2>
            <p>If you could only make progress on one this week, which matters more?</p>
          </div>
          <button
            type="button"
            className="secondary-button"
            onClick={() => setPair(nextPair(topics.length))}
          >
            <GitCompareArrows size={16} />
            New Pair
          </button>
        </div>
        <div className="mt-6 grid gap-4 md:grid-cols-2">
          {[first, second].map((topic, index) => {
            const loser = index === 0 ? second : first
            return (
              <button
                key={topic.id}
                type="button"
                className="choice-button"
                onClick={() => onChoose(topic, loser)}
              >
                <span className="text-lg font-semibold">{topic.name}</span>
                <span className="mt-2 text-sm text-slate-500">
                  Elo {Math.round(topic.elo)} · {formatHours(Math.max(0, topic.targetMinutes - topic.completedMinutes))} remaining
                </span>
              </button>
            )
          })}
        </div>
      </section>
      <section className="panel">
        <div className="section-header">
          <div>
            <h2>Current Ranking</h2>
            <p>Elo remains the stated preference signal</p>
          </div>
        </div>
        <ol className="mt-4 grid gap-2">
          {[...topics]
            .sort((a, b) => b.elo - a.elo)
            .map((topic, index) => (
              <li key={topic.id} className="flex items-center justify-between rounded border border-slate-200 bg-white px-3 py-2 text-sm">
                <span>{index + 1}. {topic.name}</span>
                <span className="font-mono text-slate-500">{Math.round(topic.elo)}</span>
              </li>
            ))}
        </ol>
      </section>
    </div>
  )
}

function PlannerView({
  draft,
  setDraft,
  preview,
  setPreview,
}: {
  draft: BootstrapData
  setDraft: (next: BootstrapData) => void
  preview: SchedulePreview | null
  setPreview: (preview: SchedulePreview | null) => void
}) {
  const [startDate, setStartDate] = useState(todayString())
  const [endDate, setEndDate] = useState(maxDeadline(draft.topics) ?? todayString())
  const [busy, setBusy] = useState(false)

  async function simulate() {
    setBusy(true)
    try {
      setPreview(
        await api.simulate({
          startDate,
          endDate,
          settings: draft.settings,
          topics: draft.topics,
          studyWindows: draft.studyWindows,
          blockedIntervals: draft.blockedIntervals,
          capacityOverrides: draft.capacityOverrides,
          lastStudiedDates: {},
        }),
      )
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="grid gap-5 xl:grid-cols-[0.8fr_1.2fr]">
      <section className="panel">
        <div className="section-header">
          <div>
            <h2>Draft Scenario</h2>
            <p>Adjust weights and horizon before applying changes</p>
          </div>
          <button type="button" className="primary-button" onClick={simulate} disabled={busy}>
            <Play size={16} />
            Simulate
          </button>
        </div>
        <div className="mt-4 grid gap-4">
          <label className="label">
            Start
            <input className="field" type="date" value={startDate} onChange={(event) => setStartDate(event.target.value)} />
          </label>
          <label className="label">
            End
            <input className="field" type="date" value={endDate} onChange={(event) => setEndDate(event.target.value)} />
          </label>
        </div>
        <WeightEditor draft={draft} setDraft={setDraft} compact />
      </section>
      <section className="panel">
        <div className="section-header">
          <div>
            <h2>Simulation Result</h2>
            <p>{preview ? `${preview.sessions.length} sessions generated` : 'No preview yet'}</p>
          </div>
        </div>
        {preview ? (
          <div className="mt-4 grid gap-4">
            <IssueList issues={preview.issues} />
            <div className="grid gap-2">
              {preview.sessions.slice(0, 12).map((session) => (
                <div key={session.id} className="rounded border border-slate-200 bg-white px-3 py-2 text-sm">
                  {session.date} · {minutesToClock(session.startMinute)}-{minutesToClock(session.endMinute)} · {session.focusName}
                </div>
              ))}
            </div>
          </div>
        ) : (
          <EmptyState text="Run a simulation to compare draft feasibility and schedule shape." />
        )}
      </section>
    </div>
  )
}

function TrackingView({
  snapshot,
  refresh,
}: {
  snapshot: AppSnapshot
  refresh: () => Promise<void>
}) {
  const [manualTopicId, setManualTopicId] = useState(snapshot.bootstrap.topics[0]?.id ?? '')
  const [manualMinutes, setManualMinutes] = useState(45)

  async function log(session: PersistedSession, status: SessionStatus, minutes: number) {
    await api.logSession(session.id, {
      topicId: session.topicId,
      date: session.date,
      minutes,
      status,
    })
    await refresh()
  }

  return (
    <div className="grid gap-5 xl:grid-cols-[1.2fr_0.8fr]">
      <section className="panel">
        <div className="section-header">
          <div>
            <h2>Planned Sessions</h2>
            <p>Complete, partially complete, or miss scheduled work</p>
          </div>
        </div>
        <div className="mt-4 grid gap-3">
          {(snapshot.currentSchedule?.sessions ?? []).slice(0, 30).map((session) => (
            <div key={session.id} className="session-row">
              <div>
                <div className="font-medium">{session.focusName}</div>
                <div className="text-xs text-slate-500">
                  {session.date} · {minutesToClock(session.startMinute)}-{minutesToClock(session.endMinute)}
                </div>
              </div>
              <div className="flex flex-wrap gap-2">
                <button type="button" className="small-button" onClick={() => log(session, 'complete', session.endMinute - session.startMinute)}>
                  Complete
                </button>
                <button type="button" className="small-button" onClick={() => log(session, 'partial', Math.round((session.endMinute - session.startMinute) / 2))}>
                  Partial
                </button>
                <button type="button" className="small-button" onClick={() => log(session, 'missed', 0)}>
                  Missed
                </button>
              </div>
            </div>
          ))}
        </div>
      </section>
      <section className="panel">
        <div className="section-header">
          <div>
            <h2>Manual Log</h2>
            <p>Record study outside the generated plan</p>
          </div>
        </div>
        <div className="mt-4 grid gap-4">
          <label className="label">
            Topic
            <select className="field" value={manualTopicId} onChange={(event) => setManualTopicId(event.target.value)}>
              {snapshot.bootstrap.topics
                .filter((topic) => !topic.archived)
                .map((topic) => (
                  <option key={topic.id} value={topic.id}>
                    {topic.name}
                  </option>
                ))}
            </select>
          </label>
          <label className="label">
            Minutes
            <input className="field" type="number" min={0} value={manualMinutes} onChange={(event) => setManualMinutes(Number(event.target.value))} />
          </label>
          <button
            type="button"
            className="primary-button"
            onClick={async () => {
              await api.manualLog({
                topicId: manualTopicId,
                date: todayString(),
                minutes: manualMinutes,
                status: 'manual',
              })
              await refresh()
            }}
          >
            <CheckCircle2 size={16} />
            Log Study
          </button>
        </div>
      </section>
    </div>
  )
}

function WindowEditor({
  title,
  description,
  windows,
  onChange,
  defaultLabel,
}: {
  title: string
  description: string
  windows: AvailabilityWindow[]
  onChange: (windows: AvailabilityWindow[]) => void
  defaultLabel: string
}) {
  function addWindow() {
    onChange([
      ...windows,
      {
        id: crypto.randomUUID(),
        kind: 'recurring',
        dayOfWeek: 1,
        date: null,
        startMinute: 18 * 60,
        endMinute: 20 * 60,
        label: defaultLabel,
      },
    ])
  }

  function updateWindow(id: string, patch: Partial<AvailabilityWindow>) {
    onChange(windows.map((window) => (window.id === id ? { ...window, ...patch } : window)))
  }

  return (
    <section className="panel">
      <div className="section-header">
        <div>
          <h2>{title}</h2>
          <p>{description}</p>
        </div>
        <button type="button" className="primary-button" onClick={addWindow}>
          <Plus size={16} />
          Add
        </button>
      </div>
      <div className="mt-4 grid gap-3">
        {windows.length === 0 ? <EmptyState text="No intervals configured." /> : null}
        {windows.map((window) => (
          <div key={window.id} className="grid gap-3 rounded border border-slate-200 bg-white p-3 md:grid-cols-[1fr_1fr_1fr_1fr_auto]">
            <label className="label">
              Kind
              <select
                className="field"
                value={window.kind}
                onChange={(event) => {
                  const kind = event.target.value as 'recurring' | 'one_off'
                  updateWindow(window.id, {
                    kind,
                    dayOfWeek: kind === 'recurring' ? 1 : null,
                    date: kind === 'one_off' ? todayString() : null,
                  })
                }}
              >
                <option value="recurring">Recurring</option>
                <option value="one_off">One-off</option>
              </select>
            </label>
            {window.kind === 'recurring' ? (
              <label className="label">
                Day
                <select
                  className="field"
                  value={window.dayOfWeek ?? 1}
                  onChange={(event) => updateWindow(window.id, { dayOfWeek: Number(event.target.value) })}
                >
                  {dayNames.map((day, index) => (
                    <option key={day} value={index + 1}>
                      {day}
                    </option>
                  ))}
                </select>
              </label>
            ) : (
              <label className="label">
                Date
                <input className="field" type="date" value={window.date ?? todayString()} onChange={(event) => updateWindow(window.id, { date: event.target.value })} />
              </label>
            )}
            <label className="label">
              Start
              <input className="field" type="time" value={minutesToClock(window.startMinute)} onChange={(event) => updateWindow(window.id, { startMinute: clockToMinutes(event.target.value) })} />
            </label>
            <label className="label">
              End
              <input className="field" type="time" value={minutesToClock(window.endMinute)} onChange={(event) => updateWindow(window.id, { endMinute: clockToMinutes(event.target.value) })} />
            </label>
            <div className="flex items-end gap-2">
              <input className="field min-w-28" value={window.label} onChange={(event) => updateWindow(window.id, { label: event.target.value })} />
              <button type="button" className="icon-button" onClick={() => onChange(windows.filter((item) => item.id !== window.id))} aria-label="Delete interval">
                <Trash2 size={16} />
              </button>
            </div>
          </div>
        ))}
      </div>
    </section>
  )
}

function WeightEditor({
  draft,
  setDraft,
  compact = false,
}: {
  draft: BootstrapData
  setDraft: (next: BootstrapData) => void
  compact?: boolean
}) {
  return (
    <div className={compact ? 'mt-4 grid gap-3' : 'mt-6 grid gap-3 md:grid-cols-2'}>
      {factorNames.map((factor) => (
        <label key={factor} className="label capitalize">
          {factor} weight
          <div className="flex items-center gap-3">
            <input
              className="w-full accent-teal-700"
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={draft.settings.priorityWeights[factor]}
              onChange={(event) =>
                setDraft({
                  ...draft,
                  settings: {
                    ...draft.settings,
                    priorityWeights: {
                      ...draft.settings.priorityWeights,
                      [factor]: Number(event.target.value),
                    },
                  },
                })
              }
            />
            <span className="w-12 text-right font-mono text-xs">{draft.settings.priorityWeights[factor].toFixed(2)}</span>
          </div>
        </label>
      ))}
    </div>
  )
}

function IssueList({ issues }: { issues: FeasibilityIssue[] }) {
  if (issues.length === 0) {
    return <div className="mt-4 rounded border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-700">No feasibility issues reported.</div>
  }

  return (
    <div className="mt-4 grid gap-2">
      {issues.map((issue, index) => (
        <div key={`${issue.code}-${index}`} className="flex gap-2 rounded border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-800">
          <AlertTriangle className="mt-0.5 shrink-0" size={16} />
          <span>{issue.message}</span>
        </div>
      ))}
    </div>
  )
}

function ProgressTopic({ topic }: { topic: Topic }) {
  const percent = topic.targetMinutes > 0 ? Math.min(100, Math.round((topic.completedMinutes / topic.targetMinutes) * 100)) : 0
  return (
    <div className="rounded border border-slate-200 bg-white p-3">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="font-medium">{topic.name}</div>
          <div className="text-xs text-slate-500">
            {formatHours(topic.completedMinutes)} / {formatHours(topic.targetMinutes)}
          </div>
        </div>
        <span className="font-mono text-xs text-slate-500">{percent}%</span>
      </div>
      <div className="mt-3 h-2 rounded bg-slate-100">
        <div className="h-2 rounded bg-teal-600" style={{ width: `${percent}%` }} />
      </div>
    </div>
  )
}

function SessionRow({ session }: { session: PersistedSession }) {
  return (
    <div className="session-row">
      <div>
        <div className="font-medium">{session.focusName}</div>
        <div className="text-xs text-slate-500">{session.topicName}</div>
      </div>
      <div className="text-right text-sm">
        <div>{minutesToClock(session.startMinute)}-{minutesToClock(session.endMinute)}</div>
        <div className="text-xs text-slate-500">{session.status}</div>
      </div>
    </div>
  )
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded border border-slate-200 bg-white p-3">
      <div className="text-xs text-slate-500">{label}</div>
      <div className="mt-1 text-lg font-semibold">{value}</div>
    </div>
  )
}

function ChecklistItem({ ok, text }: { ok: boolean; text: string }) {
  return (
    <div className="flex items-center gap-2 rounded border border-slate-200 bg-white px-3 py-2 text-sm">
      <CheckCircle2 size={16} className={ok ? 'text-emerald-600' : 'text-slate-300'} />
      {text}
    </div>
  )
}

function StatusPill({ ok }: { ok: boolean }) {
  return (
    <span className={`rounded-full px-2.5 py-1 text-xs font-medium ${ok ? 'bg-emerald-100 text-emerald-700' : 'bg-amber-100 text-amber-700'}`}>
      {ok ? 'Ready' : 'Needs Setup'}
    </span>
  )
}

function EmptyState({ text }: { text: string }) {
  return <div className="rounded border border-dashed border-slate-300 bg-white px-4 py-6 text-center text-sm text-slate-500">{text}</div>
}

function titleForView(view: ViewKey) {
  return views.find((item) => item.key === view)?.label ?? 'Study Scheduler'
}

function todayString() {
  return localDate(new Date())
}

function localDate(date: Date) {
  const adjusted = new Date(date.getTime() - date.getTimezoneOffset() * 60_000)
  return adjusted.toISOString().slice(0, 10)
}

function dateToMinute(date: Date) {
  return date.getHours() * 60 + date.getMinutes()
}

function minutesToClock(minutes: number) {
  const hours = Math.floor(minutes / 60)
  const mins = minutes % 60
  return `${String(hours).padStart(2, '0')}:${String(mins).padStart(2, '0')}`
}

function clockToMinutes(value: string) {
  const [hours, minutes] = value.split(':').map(Number)
  return hours * 60 + minutes
}

function minutesToHoursInput(minutes: number | null) {
  if (minutes === null) return ''
  return Number((minutes / 60).toFixed(2))
}

function hoursToMinutes(value: string) {
  return Math.round(Number(value || 0) * 60)
}

function hoursToNullableMinutes(value: string) {
  return value === '' ? null : hoursToMinutes(value)
}

function formatHours(minutes: number) {
  return `${(minutes / 60).toFixed(minutes % 60 === 0 ? 0 : 1)}h`
}

function sessionColor(status: SessionStatus) {
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

function normalizePair(pair: [number, number], length: number): [number, number] {
  if (length < 2) return [0, 0]
  const first = pair[0] % length
  let second = pair[1] % length
  if (first === second) second = (second + 1) % length
  return [first, second]
}

function nextPair(length: number): [number, number] {
  if (length < 2) return [0, 0]
  const first = Math.floor(Math.random() * length)
  let second = Math.floor(Math.random() * length)
  if (first === second) second = (second + 1) % length
  return [first, second]
}

function maxDeadline(topics: Topic[]) {
  return topics
    .map((topic) => topic.deadline)
    .filter((value): value is string => Boolean(value))
    .sort()
    .at(-1)
}

export default App
