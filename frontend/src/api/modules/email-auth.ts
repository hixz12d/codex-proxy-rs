import type { RequestOptions } from '../request'
import type { AccountImportSettings } from './accounts/credentials'
import request from '../request'

export type EmailAuthBatchState = 'running' | 'stopping' | 'stopped' | 'finished'

export type EmailAuthItemState = 'pending' | 'running' | 'succeeded' | 'skipped' | 'failed' | 'stopped'

export interface EmailAuthCounts {
  total: number
  pending: number
  running: number
  succeeded: number
  skipped: number
  failed: number
  stopped: number
}

export interface EmailAuthItem {
  email: string
  state: EmailAuthItemState
  code: string | null
  message: string | null
  retryable: boolean
  remoteAccountId: string | null
  updatedAt: string
}

export interface EmailAuthBatch {
  id: string
  state: EmailAuthBatchState
  createdAt: string
  finishedAt: string | null
  counts: EmailAuthCounts
  items: EmailAuthItem[]
}

export interface EmailAuthState {
  configured: boolean
  batch: EmailAuthBatch | null
}

export interface EmailAuthBatchResult {
  configured: boolean
  batch: EmailAuthBatch
}

export interface StartEmailAuthParam {
  emails: string[]
  settings?: AccountImportSettings
}

export interface RetryEmailAuthParam {
  email: string
}

export function getEmailAuth(options: RequestOptions = {}) {
  return request<EmailAuthState>({
    url: '/api/admin/accounts/email-auth',
    method: 'GET',
    ...options,
  })
}

export function startEmailAuth(data: StartEmailAuthParam, options: RequestOptions = {}) {
  return request<EmailAuthBatchResult>({
    url: '/api/admin/accounts/email-auth',
    method: 'POST',
    data,
    ...options,
  })
}

export function stopEmailAuth(options: RequestOptions = {}) {
  return request<EmailAuthBatchResult>({
    url: '/api/admin/accounts/email-auth/stop',
    method: 'POST',
    data: {},
    ...options,
  })
}

export function retryEmailAuth(data: RetryEmailAuthParam, options: RequestOptions = {}) {
  return request<EmailAuthBatchResult>({
    url: '/api/admin/accounts/email-auth/retry',
    method: 'POST',
    data,
    ...options,
  })
}
