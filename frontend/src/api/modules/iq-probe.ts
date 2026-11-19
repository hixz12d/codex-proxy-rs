import type { RequestOptions } from '../request'
import request from '../request'

export type IqProbeReasoningEffort = 'none' | 'minimal' | 'low' | 'medium' | 'high' | 'xhigh'

export interface IqProbeQuestion {
  prompt: string
  model: string
  reasoningEffort: IqProbeReasoningEffort | null
  passKeywords: string[]
  failKeywords: string[]
  maxAttempts: number
}

export interface IqProbeSettings {
  enabled: boolean
  intervalMinutes: number
  staggerMinutes: number
  recheckDelayMinutes: number
  breakerThreshold: number
  questions: IqProbeQuestion[]
}

export interface IqProbeStatus {
  breakerActive: boolean
  autoDisabledInWindow: number
  windowMinutes: number
}

export interface IqProbeSettingsView {
  settings: IqProbeSettings
  revision: number
  status: IqProbeStatus
}

export type IqProbeKind = 'scheduled' | 'recheck'
export type IqProbeOutcome = 'passed' | 'passed_after_retry' | 'failed' | 'error'
export type IqProbeAction = 'none' | 'disabled' | 'breaker_held'
export type IqProbeQuestionVerdict = IqProbeOutcome
export type IqProbeAttemptVerdict = 'pass' | 'fail' | 'unclear' | 'error'

export interface IqProbeAttempt {
  answer: string
  verdict: IqProbeAttemptVerdict
  error: string | null
  elapsedMs: number
}

export interface IqProbeQuestionResult {
  prompt: string
  model: string
  reasoningEffort: string | null
  verdict: IqProbeQuestionVerdict
  attempts: IqProbeAttempt[]
}

export interface IqProbeRound {
  id: string
  accountId: string
  kind: IqProbeKind
  startedAt: string
  finishedAt: string
  outcome: IqProbeOutcome
  action: IqProbeAction
  detail: { questions: IqProbeQuestionResult[] }
}

interface IqProbeSettingsUpdateParam {
  settings: IqProbeSettings
  revision: number
}

interface IqProbeRoundsResponse {
  items: IqProbeRound[]
}

export function getIqProbeSettings(options: RequestOptions = {}) {
  return request<IqProbeSettingsView>({
    url: '/api/admin/iq-probe/settings',
    method: 'GET',
    ...options,
  })
}

export function updateIqProbeSettings(data: IqProbeSettingsUpdateParam, options: RequestOptions = {}) {
  return request<IqProbeSettingsView>({
    url: '/api/admin/iq-probe/settings/update',
    method: 'POST',
    data,
    ...options,
  })
}

/** 每个账号最多返回 24 轮，新的在前；一次最多 200 个账号 */
export function getIqProbeRounds(accountIds: string[], options: RequestOptions = {}) {
  return request<IqProbeRoundsResponse>({
    url: '/api/admin/iq-probe/rounds',
    method: 'GET',
    params: {
      accountIds: accountIds.join(','),
    },
    ...options,
  })
}
