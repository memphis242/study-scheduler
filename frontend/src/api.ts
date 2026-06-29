import type {
  AppSettings,
  AppSnapshot,
  BootstrapData,
  GenerateScheduleResponse,
  PersistedSession,
  PriorityComparisonResponse,
  ScheduleInput,
  SchedulePreview,
  SessionStatus,
} from './types'

const API_BASE = import.meta.env.VITE_API_URL ?? 'http://127.0.0.1:5174'

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers: {
      'content-type': 'application/json',
      ...(init?.headers ?? {}),
    },
  })

  if (!response.ok) {
    const body = await response.json().catch(() => ({}))
    throw new Error(body.error ?? `Request failed with ${response.status}`)
  }

  return response.json() as Promise<T>
}

export const api = {
  bootstrap: () => request<AppSnapshot>('/api/bootstrap'),
  saveBootstrap: (payload: BootstrapData) =>
    request<AppSnapshot>('/api/bootstrap', {
      method: 'PUT',
      body: JSON.stringify(payload),
    }),
  saveSettings: (payload: AppSettings) =>
    request<AppSnapshot>('/api/settings', {
      method: 'PUT',
      body: JSON.stringify(payload),
    }),
  comparePriority: (
    winnerTopicId: string,
    loserTopicId: string,
    kFactor = 32,
  ) =>
    request<PriorityComparisonResponse>('/api/priority/comparisons', {
      method: 'POST',
      body: JSON.stringify({ winnerTopicId, loserTopicId, kFactor }),
    }),
  generateSchedule: (payload: {
    startDate?: string
    endDate?: string
    persist?: boolean
  }) =>
    request<GenerateScheduleResponse>('/api/schedules/generate', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  simulate: (payload: ScheduleInput) =>
    request<SchedulePreview>('/api/planner/simulate', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  pinSchedule: (id: string, name?: string) =>
    request<AppSnapshot>(`/api/schedules/${id}/pin`, {
      method: 'POST',
      body: JSON.stringify({ name }),
    }),
  updateSession: (
    id: string,
    payload: {
      date: string
      startMinute: number
      endMinute: number
      locked: boolean
      status: SessionStatus
    },
  ) =>
    request<PersistedSession>(`/api/sessions/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  postponeSession: (
    id: string,
    payload: {
      date: string
      startMinute: number
      endMinute: number
      locked?: boolean
    },
  ) =>
    request<PersistedSession>(`/api/sessions/${id}/postpone`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  logSession: (
    id: string,
    payload: {
      topicId: string
      date: string
      minutes: number
      note?: string
      status?: SessionStatus
    },
  ) =>
    request<AppSnapshot>(`/api/sessions/${id}/log`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  manualLog: (payload: {
    topicId: string
    date: string
    minutes: number
    note?: string
    status?: SessionStatus
  }) =>
    request<AppSnapshot>('/api/logs', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
}
