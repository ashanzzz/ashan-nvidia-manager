import type {
  NvidiaAccount,
  NvidiaAccountPayload,
  NvidiaModelItem,
  Settings,
  SyncRun,
  SyncRunLog,
  SystemStatus,
} from './types'

async function request<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...init?.headers,
    },
  })
  if (!res.ok) {
    const text = await res.text()
    try {
      const errJson = JSON.parse(text)
      throw new Error(errJson.error || errJson.message || text)
    } catch {
      throw new Error(`HTTP ${res.status}: ${text}`)
    }
  }
  return res.json()
}

export const api = {
  getStatus: () => request<SystemStatus>('/api/status'),
  getAccounts: () => request<NvidiaAccount[]>('/api/accounts'),
  saveAccount: (payload: NvidiaAccountPayload) =>
    request<{ status: string; id: string }>('/api/accounts', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  deleteAccount: (id: string) =>
    request<{ status: string }>(`/api/accounts/${id}`, { method: 'DELETE' }),
  probeAccount: (id: string) =>
    request<{
      status: string
      is_healthy: boolean
      detail: string
      model_latencies: [string, number][]
    }>(`/api/accounts/${id}/probe`, { method: 'POST' }),

  getModels: () => request<NvidiaModelItem[]>('/api/models'),
  getSettings: () => request<Settings>('/api/settings'),
  saveSettings: (settings: Settings) =>
    request<{ status: string }>('/api/settings', {
      method: 'POST',
      body: JSON.stringify(settings),
    }),

  startSync: () =>
    request<{ status: string; run_id: string }>('/api/sync/start', {
      method: 'POST',
    }),
  getSyncRuns: () => request<SyncRun[]>('/api/sync/runs'),
  getSyncRun: (id: string) => request<SyncRun>(`/api/sync/${id}`),
  getSyncLogs: (id: string) => request<SyncRunLog[]>(`/api/sync/${id}/logs`),
}
