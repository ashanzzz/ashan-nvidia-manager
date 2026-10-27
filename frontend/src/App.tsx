import React, { useEffect, useState } from 'react'
import { api } from './api'
import type {
  NvidiaAccount,
  NvidiaAccountPayload,
  NvidiaModelItem,
  Settings,
  SyncRun,
  SyncRunLog,
  SystemStatus,
} from './types'
import './styles.css'

export default function App() {
  const [tab, setTab] = useState<'accounts' | 'models' | 'sync' | 'settings'>('accounts')
  const [status, setStatus] = useState<SystemStatus | null>(null)
  const [accounts, setAccounts] = useState<NvidiaAccount[]>([])
  const [models, setModels] = useState<NvidiaModelItem[]>([])
  const [settings, setSettings] = useState<Settings | null>(null)
  const [runs, setRuns] = useState<SyncRun[]>([])
  const [activeRunId, setActiveRunId] = useState<string | null>(null)
  const [activeLogs, setActiveLogs] = useState<SyncRunLog[]>([])

  // Modal State
  const [modalOpen, setModalOpen] = useState(false)
  const [editingAccount, setEditingAccount] = useState<NvidiaAccountPayload>({
    name: '',
    api_key: '',
    priority: 0,
    weight: 0,
    enabled: true,
  })

  const [loading, setLoading] = useState(false)
  const [errorMsg, setErrorMsg] = useState<string | null>(null)

  const refreshAll = async () => {
    try {
      const [st, accs, mods, sett, rns] = await Promise.all([
        api.getStatus(),
        api.getAccounts(),
        api.getModels(),
        api.getSettings(),
        api.getSyncRuns(),
      ])
      setStatus(st)
      setAccounts(accs)
      setModels(mods)
      setSettings(sett)
      setRuns(rns)
      if (rns.length > 0 && !activeRunId) {
        setActiveRunId(rns[0].id)
      }
    } catch (err: any) {
      setErrorMsg(err.message || '加载数据失败')
    }
  }

  useEffect(() => {
    refreshAll()
    const timer = setInterval(() => {
      api.getStatus().then(setStatus).catch(() => {})
      api.getAccounts().then(setAccounts).catch(() => {})
    }, 10000)
    return () => clearInterval(timer)
  }, [])

  // Poll active run logs if running
  useEffect(() => {
    if (!activeRunId) return
    let cancelled = false

    const fetchLogs = async () => {
      try {
        const logs = await api.getSyncLogs(activeRunId)
        if (!cancelled) setActiveLogs(logs)
      } catch {}
    }

    fetchLogs()
    const interval = setInterval(fetchLogs, 3000)
    return () => {
      cancelled = true
      clearInterval(interval)
    }
  }, [activeRunId])

  const handleStartSync = async () => {
    setLoading(true)
    setErrorMsg(null)
    try {
      const res = await api.startSync()
      setActiveRunId(res.run_id)
      setTab('sync')
      await refreshAll()
    } catch (err: any) {
      setErrorMsg(err.message)
    } finally {
      setLoading(false)
    }
  }

  const handleProbeAccount = async (id: string) => {
    try {
      const res = await api.probeAccount(id)
      alert(`探测完成：${res.status} (${res.detail})`)
      const updated = await api.getAccounts()
      setAccounts(updated)
    } catch (err: any) {
      alert(`探测失败：${err.message}`)
    }
  }

  const handleDeleteAccount = async (id: string, name: string) => {
    if (!confirm(`确定删除账号 [${name}] 吗？`)) return
    try {
      await api.deleteAccount(id)
      await refreshAll()
    } catch (err: any) {
      alert(`删除失败: ${err.message}`)
    }
  }

  const handleSaveAccount = async () => {
    if (!editingAccount.name.trim()) {
      alert('账号名称不能为空')
      return
    }
    setLoading(true)
    try {
      await api.saveAccount(editingAccount)
      setModalOpen(false)
      await refreshAll()
    } catch (err: any) {
      alert(`保存失败: ${err.message}`)
    } finally {
      setLoading(false)
    }
  }

  const handleSaveSettings = async () => {
    if (!settings) return
    setLoading(true)
    try {
      await api.saveSettings(settings)
      alert('设置已保存并生效')
    } catch (err: any) {
      alert(`保存失败: ${err.message}`)
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="container">
      {/* Header */}
      <header>
        <div className="brand">
          <div className="brand-icon">NV</div>
          <div>
            <h1>Ashan NVIDIA Manager</h1>
            <div style={{ fontSize: '12px', color: 'var(--text-muted)' }}>
              多账号统一管理 · 实时连通性探测 · 智力排位优选 · New API 多级渠道同步
            </div>
          </div>
        </div>

        <div className="header-stats">
          <div className="stat-pill">
            总账号: <strong>{status?.total_accounts ?? 0}</strong>
          </div>
          <div className="stat-pill">
            健康在线: <strong>{status?.healthy_accounts ?? 0}</strong>
          </div>
          <div className="stat-pill">
            自动调度: <strong>{status?.scheduler?.is_running ? '运行中' : '未开启'}</strong>
          </div>
          <button
            className="btn btn-primary"
            onClick={handleStartSync}
            disabled={loading}
          >
            {loading ? '同步中...' : '立即同步'}
          </button>
        </div>
      </header>

      {errorMsg && (
        <div style={{ padding: '12px', background: 'var(--danger-alpha)', color: 'var(--danger-red)', borderRadius: '6px', marginBottom: '16px' }}>
          {errorMsg}
        </div>
      )}

      {/* Nav Tabs */}
      <nav className="nav-tabs">
        <button
          className={`tab-btn ${tab === 'accounts' ? 'active' : ''}`}
          onClick={() => setTab('accounts')}
        >
          账号管理 ({accounts.length})
        </button>
        <button
          className={`tab-btn ${tab === 'models' ? 'active' : ''}`}
          onClick={() => setTab('models')}
        >
          模型智力与上下文限制 ({models.length})
        </button>
        <button
          className={`tab-btn ${tab === 'sync' ? 'active' : ''}`}
          onClick={() => setTab('sync')}
        >
          同步控制台与记录
        </button>
        <button
          className={`tab-btn ${tab === 'settings' ? 'active' : ''}`}
          onClick={() => setTab('settings')}
        >
          系统设置
        </button>
      </nav>

      {/* Tab: Accounts */}
      {tab === 'accounts' && (
        <section>
          <div className="card">
            <div className="card-header">
              <div className="card-title">NVIDIA 账号池 (独立多渠道映射)</div>
              <button
                className="btn btn-primary"
                onClick={() => {
                  setEditingAccount({
                    name: '',
                    api_key: '',
                    priority: 0,
                    weight: 0,
                    enabled: true,
                  })
                  setModalOpen(true)
                }}
              >
                + 添加账号
              </button>
            </div>

            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>账号名称</th>
                    <th>API 密钥</th>
                    <th>New API 渠道 ID</th>
                    <th>优先级</th>
                    <th>权重</th>
                    <th>健康状态</th>
                    <th>最近探测结果</th>
                    <th>操作</th>
                  </tr>
                </thead>
                <tbody>
                  {accounts.length === 0 ? (
                    <tr>
                      <td colSpan={8} style={{ textAlign: 'center', color: 'var(--text-muted)' }}>
                        暂未配置任何账号，请点击上方「+ 添加账号」
                      </td>
                    </tr>
                  ) : (
                    accounts.map((acc) => (
                      <tr key={acc.id}>
                        <td>
                          <strong>{acc.name}</strong>
                          {!acc.enabled && <span className="badge badge-yellow" style={{ marginLeft: '6px' }}>已禁用</span>}
                        </td>
                        <td style={{ fontFamily: 'monospace', fontSize: '13px' }}>{acc.api_key}</td>
                        <td>
                          {acc.newapi_channel_id ? (
                            <span className="badge badge-blue">#{acc.newapi_channel_id}</span>
                          ) : (
                            <span style={{ color: 'var(--text-muted)' }}>未绑定</span>
                          )}
                        </td>
                        <td>{acc.priority}</td>
                        <td>{acc.weight}</td>
                        <td>
                          {acc.status === 'healthy' ? (
                            <span className="badge badge-green">在线正常</span>
                          ) : acc.status === 'rate_limited' ? (
                            <span className="badge badge-yellow">429 限流</span>
                          ) : acc.status === 'unauthorized' ? (
                            <span className="badge badge-red">鉴权失败</span>
                          ) : (
                            <span className="badge badge-yellow">{acc.status}</span>
                          )}
                        </td>
                        <td style={{ fontSize: '12px', color: 'var(--text-muted)', maxWidth: '240px' }}>
                          {acc.last_probe_status || '尚未探测'}
                        </td>
                        <td>
                          <div style={{ display: 'flex', gap: '8px' }}>
                            <button
                              className="btn"
                              style={{ padding: '4px 8px', fontSize: '12px' }}
                              onClick={() => handleProbeAccount(acc.id)}
                            >
                              测试
                            </button>
                            <button
                              className="btn"
                              style={{ padding: '4px 8px', fontSize: '12px' }}
                              onClick={() => {
                                setEditingAccount({
                                  name: acc.name,
                                  api_key: '',
                                  newapi_channel_id: acc.newapi_channel_id,
                                  priority: acc.priority,
                                  weight: acc.weight,
                                  enabled: acc.enabled,
                                })
                                setModalOpen(true)
                              }}
                            >
                              编辑
                            </button>
                            <button
                              className="btn btn-danger"
                              style={{ padding: '4px 8px', fontSize: '12px' }}
                              onClick={() => handleDeleteAccount(acc.id, acc.name)}
                            >
                              删除
                            </button>
                          </div>
                        </td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>
          </div>
        </section>
      )}

      {/* Tab: Models */}
      {tab === 'models' && (
        <section>
          <div className="grid-models">
            {models.map((m) => (
              <div className="model-card" key={m.id}>
                <div className="model-header">
                  <div>
                    <div className="model-name">{m.name}</div>
                    <div className="model-id">{m.id}</div>
                  </div>
                  <span
                    className={`badge ${
                      m.category === 'reasoning'
                        ? 'badge-purple'
                        : m.category === 'multimodal'
                        ? 'badge-blue'
                        : 'badge-green'
                    }`}
                  >
                    {m.category === 'reasoning'
                      ? '旗舰推理'
                      : m.category === 'multimodal'
                      ? '多模态'
                      : '极速吞吐'}
                  </span>
                </div>

                <div className="spec-row">
                  <span>架构机制</span>
                  <span className="spec-val">{m.architecture}</span>
                </div>

                <div className="spec-row">
                  <span>上下文窗口 (Context)</span>
                  <span className="spec-val" style={{ color: 'var(--nvidia-green)' }}>
                    {m.context_length >= 1048576
                      ? '1,048,576 tokens (1M)'
                      : `${m.context_length.toLocaleString()} tokens`}
                  </span>
                </div>

                <div className="spec-row">
                  <span>单次最大输出</span>
                  <span className="spec-val">{m.max_output_tokens.toLocaleString()} tokens</span>
                </div>

                <div className="spec-row">
                  <span>模型规模 (总 / 激活)</span>
                  <span className="spec-val">
                    {m.total_parameters} / {m.active_parameters}
                  </span>
                </div>

                <div className="spec-row">
                  <span>SWE-Bench 评分</span>
                  <span className="spec-val">
                    {m.benchmark_swe ? `${m.benchmark_swe}%` : 'N/A'}
                  </span>
                </div>

                <div className="spec-row">
                  <span>Terminal-Bench 评分</span>
                  <span className="spec-val">
                    {m.benchmark_terminal ? `${m.benchmark_terminal}%` : 'N/A'}
                  </span>
                </div>
              </div>
            ))}
          </div>
        </section>
      )}

      {/* Tab: Sync */}
      {tab === 'sync' && (
        <section>
          <div className="card">
            <div className="card-header">
              <div className="card-title">同步日志跟踪 (Run: {activeRunId || '无'})</div>
              <button
                className="btn btn-primary"
                onClick={handleStartSync}
                disabled={loading}
              >
                + 手动触发全量同步
              </button>
            </div>

            <div className="log-console">
              {activeLogs.length === 0 ? (
                <div style={{ color: 'var(--text-muted)' }}>暂无日志记录</div>
              ) : (
                activeLogs.map((l) => (
                  <div className="log-line" key={l.id}>
                    <span className="log-time">[{new Date(l.created_at).toLocaleTimeString()}]</span>
                    <span className={`log-${l.level}`}>[{l.stage.toUpperCase()}]</span>
                    <span>{l.message}</span>
                    {l.detail && <span style={{ color: 'var(--text-muted)' }}>({l.detail})</span>}
                  </div>
                ))
              )}
            </div>
          </div>

          <div className="card">
            <div className="card-title" style={{ marginBottom: '16px' }}>历史同步记录</div>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>任务 ID</th>
                    <th>开始时间</th>
                    <th>完成时间</th>
                    <th>状态</th>
                    <th>候选模型数</th>
                    <th>操作</th>
                  </tr>
                </thead>
                <tbody>
                  {runs.map((r) => (
                    <tr key={r.id}>
                      <td style={{ fontFamily: 'monospace' }}>{r.id.substring(0, 8)}...</td>
                      <td>{new Date(r.started_at).toLocaleString()}</td>
                      <td>{r.finished_at ? new Date(r.finished_at).toLocaleString() : '进行中'}</td>
                      <td>
                        <span
                          className={`badge ${
                            r.status === 'success'
                              ? 'badge-green'
                              : r.status === 'running'
                              ? 'badge-blue'
                              : 'badge-red'
                          }`}
                        >
                          {r.status}
                        </span>
                      </td>
                      <td>{r.candidate_models.length}</td>
                      <td>
                        <button
                          className="btn"
                          style={{ padding: '2px 8px', fontSize: '12px' }}
                          onClick={() => setActiveRunId(r.id)}
                        >
                          查看日志
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </section>
      )}

      {/* Tab: Settings */}
      {tab === 'settings' && settings && (
        <section>
          <div className="card" style={{ maxWidth: '800px' }}>
            <div className="card-title" style={{ marginBottom: '20px' }}>系统集成与同步设置</div>

            <div className="form-group">
              <label className="form-label">New API 地址 (Base URL)</label>
              <input
                className="form-input"
                value={settings.newapi_base_url}
                onChange={(e) => setSettings({ ...settings, newapi_base_url: e.target.value })}
              />
            </div>

            <div className="form-group">
              <label className="form-label">New API 管理员用户 ID</label>
              <input
                className="form-input"
                value={settings.newapi_admin_user_id}
                onChange={(e) => setSettings({ ...settings, newapi_admin_user_id: e.target.value })}
              />
            </div>

            <div className="form-group">
              <label className="form-label">NVIDIA NIM API Base URL</label>
              <input
                className="form-input"
                value={settings.nvidia_api_base}
                onChange={(e) => setSettings({ ...settings, nvidia_api_base: e.target.value })}
              />
            </div>

            <div className="form-group">
              <label className="form-label">统一别名模型 (Alias Model Name)</label>
              <input
                className="form-input"
                value={settings.alias_model}
                onChange={(e) => setSettings({ ...settings, alias_model: e.target.value })}
              />
              <div style={{ fontSize: '12px', color: 'var(--text-muted)', marginTop: '4px' }}>
                所有受管渠道都会暴露该公共别名，同时保留各自实际的官方模型 ID。
              </div>
            </div>

            <div className="form-group">
              <label className="form-label">渠道分组 (Group)</label>
              <input
                className="form-input"
                value={settings.managed_group}
                onChange={(e) => setSettings({ ...settings, managed_group: e.target.value })}
              />
            </div>

            <div className="form-group">
              <label className="form-label">渠道标识前缀 (Prefix)</label>
              <input
                className="form-input"
                value={settings.channel_name_prefix}
                onChange={(e) => setSettings({ ...settings, channel_name_prefix: e.target.value })}
              />
            </div>

            <div className="form-group" style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <input
                type="checkbox"
                checked={settings.auto_sync}
                id="auto_sync"
                onChange={(e) => setSettings({ ...settings, auto_sync: e.target.checked })}
              />
              <label htmlFor="auto_sync" style={{ fontSize: '14px', cursor: 'pointer' }}>
                启用后台自动调度与健康检测轮转
              </label>
            </div>

            {settings.auto_sync && (
              <div className="form-group">
                <label className="form-label">调度间隔 (分钟)</label>
                <input
                  type="number"
                  className="form-input"
                  value={settings.sync_interval_minutes}
                  onChange={(e) => setSettings({ ...settings, sync_interval_minutes: parseInt(e.target.value) || 60 })}
                />
              </div>
            )}

            <div style={{ marginTop: '24px' }}>
              <button className="btn btn-primary" onClick={handleSaveSettings} disabled={loading}>
                保存设置
              </button>
            </div>
          </div>
        </section>
      )}

      {/* Account Modal */}
      {modalOpen && (
        <div className="modal-backdrop">
          <div className="modal">
            <h3 style={{ marginBottom: '16px' }}>
              {editingAccount.name ? '编辑账号' : '添加 NVIDIA 账号'}
            </h3>

            <div className="form-group">
              <label className="form-label">账号备注名称</label>
              <input
                className="form-input"
                placeholder="例如: nvidia_13602004317 或 ashan-pro"
                value={editingAccount.name}
                onChange={(e) => setEditingAccount({ ...editingAccount, name: e.target.value })}
              />
            </div>

            <div className="form-group">
              <label className="form-label">NVIDIA API Key</label>
              <input
                type="password"
                className="form-input"
                placeholder="nvapi-..."
                value={editingAccount.api_key || ''}
                onChange={(e) => setEditingAccount({ ...editingAccount, api_key: e.target.value })}
              />
              <div style={{ fontSize: '12px', color: 'var(--text-muted)', marginTop: '4px' }}>
                密钥将使用 AES-GCM 密文存储，若无需修改 API Key 可留空。
              </div>
            </div>

            <div className="form-group">
              <label className="form-label">绑定已有 New API 渠道 ID (选填)</label>
              <input
                type="number"
                className="form-input"
                placeholder="例如: 7"
                value={editingAccount.newapi_channel_id || ''}
                onChange={(e) =>
                  setEditingAccount({
                    ...editingAccount,
                    newapi_channel_id: e.target.value ? parseInt(e.target.value) : undefined,
                  })
                }
              />
            </div>

            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px' }}>
              <div className="form-group">
                <label className="form-label">渠道优先级 (Priority)</label>
                <input
                  type="number"
                  className="form-input"
                  value={editingAccount.priority ?? 0}
                  onChange={(e) =>
                    setEditingAccount({
                      ...editingAccount,
                      priority: parseInt(e.target.value) || 0,
                    })
                  }
                />
              </div>

              <div className="form-group">
                <label className="form-label">渠道权重 (Weight)</label>
                <input
                  type="number"
                  className="form-input"
                  value={editingAccount.weight ?? 0}
                  onChange={(e) =>
                    setEditingAccount({
                      ...editingAccount,
                      weight: parseInt(e.target.value) || 0,
                    })
                  }
                />
              </div>
            </div>

            <div className="form-group" style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <input
                type="checkbox"
                id="acc_enabled"
                checked={editingAccount.enabled ?? true}
                onChange={(e) => setEditingAccount({ ...editingAccount, enabled: e.target.checked })}
              />
              <label htmlFor="acc_enabled" style={{ fontSize: '14px', cursor: 'pointer' }}>
                启用该账号
              </label>
            </div>

            <div className="modal-footer">
              <button className="btn" onClick={() => setModalOpen(false)}>
                取消
              </button>
              <button className="btn btn-primary" onClick={handleSaveAccount} disabled={loading}>
                保存
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
