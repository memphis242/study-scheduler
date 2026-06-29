import { afterEach, describe, expect, it, vi } from 'vitest'

import { api } from './api'

function mockFetchJson(payload: unknown, init: { ok?: boolean; status?: number } = {}) {
  const fetchMock = vi.fn().mockResolvedValue({
    ok: init.ok ?? true,
    status: init.status ?? 200,
    json: vi.fn().mockResolvedValue(payload),
  })
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

describe('api client', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  it('loads bootstrap from the backend base URL', async () => {
    const payload = { bootstrap: { topics: [] } }
    const fetchMock = mockFetchJson(payload)

    await expect(api.bootstrap()).resolves.toEqual(payload)
    expect(fetchMock).toHaveBeenCalledWith(
      'http://127.0.0.1:5174/api/bootstrap',
      expect.objectContaining({
        headers: expect.objectContaining({ 'content-type': 'application/json' }),
      }),
    )
  })

  it('serializes priority comparisons with camelCase payload keys', async () => {
    const fetchMock = mockFetchJson({ update: {}, topics: [] })

    await api.comparePriority('winner', 'loser', 16)

    expect(fetchMock).toHaveBeenCalledWith(
      'http://127.0.0.1:5174/api/priority/comparisons',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({
          winnerTopicId: 'winner',
          loserTopicId: 'loser',
          kFactor: 16,
        }),
      }),
    )
  })

  it('updates sessions with PATCH and the expected JSON body', async () => {
    const payload = {
      id: 'session',
      date: '2026-06-29',
      startMinute: 600,
      endMinute: 660,
      locked: true,
      status: 'locked',
    }
    const fetchMock = mockFetchJson(payload)

    await expect(
      api.updateSession('session', {
        date: '2026-06-29',
        startMinute: 600,
        endMinute: 660,
        locked: true,
        status: 'locked',
      }),
    ).resolves.toEqual(payload)

    expect(fetchMock).toHaveBeenCalledWith(
      'http://127.0.0.1:5174/api/sessions/session',
      expect.objectContaining({
        method: 'PATCH',
        body: JSON.stringify({
          date: '2026-06-29',
          startMinute: 600,
          endMinute: 660,
          locked: true,
          status: 'locked',
        }),
      }),
    )
  })

  it('throws backend error messages when requests fail with JSON errors', async () => {
    mockFetchJson({ error: 'missing deadline' }, { ok: false, status: 500 })

    await expect(api.generateSchedule({ persist: true })).rejects.toThrow('missing deadline')
  })

  it('throws a status-based fallback when an error response is not JSON', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: false,
      status: 503,
      json: vi.fn().mockRejectedValue(new Error('not json')),
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(api.bootstrap()).rejects.toThrow('Request failed with 503')
  })
})
