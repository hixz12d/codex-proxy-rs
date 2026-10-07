import type { RequestOptions } from '../request'
import type { AccountGroupRef } from './account-groups'
import { toast } from '@codex-proxy/ui'
import { API_BASE_URL } from '../constants'
import { ApiError } from '../error'
import request from '../request'

export type AccountStatus
  = 'normal' | 'quota_exhausted' | 'rate_limited' | 'disabled' | 'error'

export type AccountErrorReason
  = 'account_unverified'
    | 'access_token_expired'
    | 'credential_expired'
    | 'credential_invalid'
    | 'account_banned'

export interface AccountQuotaWindow {
  key: string
  group: string
  limitId: string | null
  limitName: string | null
  role: 'primary' | 'secondary' | 'monthly' | null
  windowSeconds: number | null
  labelDisplay: string
  windowLabelDisplay: string
  usedPercent: number | null
  usedPercentDisplay: string
  limitReached: boolean
  localUsage?: unknown
  resetAtDisplay: string
}

export interface AccountQuotaCredits {
  hasCredits: boolean
  unlimited: boolean
  balance: string | null
}

export interface AccountQuota {
  refreshedAtDisplay: string
  limitReached: boolean
  // 429 临时限流（Redis 冷却）到期时间；非限流中为 null。
  rateLimitedUntil: string | null
  rateLimitRecoveryDisplay: string | null
  rateLimitReason: 'upstream_rate_limit' | 'capacity_freeze' | null
  recoveryProbeRequired: boolean
  windows: AccountQuotaWindow[]
  credits: AccountQuotaCredits | null
}

export interface AccountCurrencyCost {
  currency: string
  estimatedAmount: string
  estimatedAmountDisplay: string
}

export interface AccountModelUsage {
  model: string
  requestCount: number
  requestCountDisplay: string
  successRate: number | null
  successRateDisplay: string
  inputTokens: number | null
  inputTokensDisplay: string
  outputTokens: number | null
  outputTokensDisplay: string
  cachedTokens: number | null
  cachedTokensDisplay: string
  imageInputTokens: number | null
  imageInputTokensDisplay: string
  imageOutputTokens: number | null
  imageOutputTokensDisplay: string
  imageRequestCount: number
  imageRequestCountDisplay: string
  imageRequestFailedCount: number
  imageRequestFailedCountDisplay: string
  totalTokens: number | null
  totalTokensDisplay: string
  billingAmountUsd: string | null
  billingAmountUsdDisplay: string
  costEstimateStatus: string
  knownCostCount: number
  partialCostCount: number
  unknownCostCount: number
  costs: AccountCurrencyCost[]
  lastUsedAt: string
  lastUsedAtDisplay: string
  lastUsedAtFullDisplay: string | null
}

export interface AccountUsage {
  windowLabelDisplay: string
  requestCount: number | null
  requestCountDisplay: string
  inputTokens: number | null
  inputTokensDisplay: string
  outputTokens: number | null
  outputTokensDisplay: string
  cachedTokens: number | null
  cachedTokensDisplay: string
  reasoningTokens: number | null
  reasoningTokensDisplay: string
  imageInputTokens: number | null
  imageInputTokensDisplay: string
  imageOutputTokens: number | null
  imageOutputTokensDisplay: string
  imageRequestCount: number | null
  imageRequestCountDisplay: string
  imageRequestFailedCount: number | null
  imageRequestFailedCountDisplay: string
  totalTokens: number | null
  totalTokensDisplay: string
  createdTokens: number | null
  createdTokensDisplay: string
  readTokens: number | null
  readTokensDisplay: string
  lastUsedAt: string | null
  lastUsedAtDisplay: string
  lastUsedAtFullDisplay: string | null
  costEstimateStatus: string
  knownCostCount: number | null
  partialCostCount: number | null
  unknownCostCount: number | null
  costs: AccountCurrencyCost[]
  models: AccountModelUsage[]
}

export interface AccountModelAccess {
  mode: 'all' | 'allowlist' | 'denylist'
  models: string[]
}

export interface Account {
  capabilities: AccountCapabilities
  outboundProxyEndpoint: string | null
  id: string
  name: string
  notes: string | null
  provider: string
  resourceRef: string
  email: string | null
  accountId: string | null
  userId: string | null
  label: string | null
  planType: string | null
  planTypeDisplay: string
  authenticationKind: string
  hasRefreshToken: boolean
  status: AccountStatus
  errorReason: AccountErrorReason | null
  errorMessage: string | null
  enabled: boolean
  concurrencyLimit: number | null
  capacity: {
    usedSlots: number | null
    totalSlots: number | null
  }
  weight: number
  modelAccess: AccountModelAccess
  accessTokenExpiresAt: string | null
  accessTokenExpiresAtDisplay: string | null
  refreshTokenExpiresAt: string | null
  nextRefreshAt: string | null
  nextRefreshAtDisplay: string | null
  addedAt: string
  addedAtDisplay: string
  updatedAt: string
  updatedAtDisplay: string
  quota: AccountQuota
  usage: AccountUsage
  groups: AccountGroupRef[]
}

export interface AccountCapabilities {
  quota: boolean
  quotaRefresh: boolean
  profile: boolean
  subscription: boolean
  avatar: boolean
  resetCredits: boolean
  consumeResetCredit: boolean
}

export interface AccountQuotaForecast {
  period: 'weekly' | 'monthly'
  targetDays: number
  extrapolated: boolean
  source: {
    label: string
    usedPercent: number | null
    usedPercentDisplay: string
    observedAt: string | null
    observedAtDisplay: string
    resetAt: string
    tokensDisplay: string
    usdDisplay: string
  } | null
  unavailableReason: string | null
  lowSample: boolean
  incompleteCost: boolean
  incompleteTokens: boolean
  estimatedTokens: number | null
  estimatedTokensDisplay: string
  estimatedUsd: number | null
  estimatedUsdDisplay: string
}

export interface AccountQuotaForecastResponse {
  accountId: string
  generatedAt: string
  forecasts: AccountQuotaForecast[]
}

export interface AccountPageMeta {
  page: number
  pageSize: number
  total: number
  totalPages: number
}

export interface AccountSummary {
  total: number
  normal: number
  quotaExhausted: number
  rateLimited: number
  disabled: number
  error: number
}

export interface AccountListResponse {
  items: Account[]
  page: AccountPageMeta
  summary: AccountSummary
}

export interface AccountRefreshResponse {
  account: Account
  result?: string
  error?: string
}

export interface AccountQuotaResponse {
  account: Account
}

export interface AccountProfileStatisticsSummary {
  totalTextTokens: number | null
  peakTokens: number | null
  longestTaskDurationMs: number | null
  currentStreakDays: number | null
  longestStreakDays: number | null
}

export interface AccountProfileDailyUsage {
  date: string
  tokens: number
}

export interface AccountProfileInvocation {
  type: string
  pluginId: string | null
  pluginName: string | null
  skillId: string | null
  skillName: string | null
  usageCount: number | null
}

export interface AccountProfileActivityInsights {
  fastModePercent: number | null
  reasoningEffort: string | null
  reasoningEffortPercent: number | null
  skillsExplored: number | null
  totalSkillsUsed: number | null
  totalThreads: number | null
  invocations: AccountProfileInvocation[] | null
}

export interface AccountSubscription {
  startsAt: string | null
  startsAtDisplay: string | null
  expiresAt: string
  expiresAtDisplay: string
  willRenew: boolean | null
  billingPeriod: string | null
  billingCurrency: string | null
  observedAt: string
  observedAtDisplay: string
}

export interface ProfileActivityCalendar {
  rangeLabel: string
  weeks: Array<{ key: string, monthLabel: string | null, cells: Array<{ date: string, dateDisplay: string, tokens: number, isFuture: boolean }> }>
}

export interface AccountProfileStatisticsResponse {
  activityCalendar: ProfileActivityCalendar | null
  displayName: string | null
  username: string | null
  imageUrl: string | null
  hasStatsError: boolean
  summary: AccountProfileStatisticsSummary
  dailyUsage: AccountProfileDailyUsage[] | null
  activityInsights: AccountProfileActivityInsights
}

export interface AccountPersonalInfoResponse {
  profile: AccountProfileStatisticsResponse | null
  profileError: string | null
  subscription: AccountSubscription | null
}

export interface AccountResetCredit {
  id: string
  status: string | null
  title: string | null
  expiresAt: string | null
  expiresAtDisplay: string | null
  resetType: string | null
}

export interface AccountResetCreditsResponse {
  availableCount: number
  credits: AccountResetCredit[]
}

export interface AccountResetCreditResultResponse {
  code: string
  credit: AccountResetCredit | null
}

export interface AccountModelsResponse {
  models: Array<{ id: string, label: string }>
}

export interface AccountModelCatalogResponse {
  modelCount: number
  observedAt: string
  /** Codex `model_catalog_json` 的文件正文，原样落盘即可被客户端加载。 */
  catalog: unknown
}

export interface AccountImportResponse {
  importedCount: number
  accountIds: string[]
}

export type ImportItemStatus = 'pending' | 'running' | 'succeeded' | 'failed' | 'unknown' | 'skipped'

export interface AccountImportTask {
  taskId: string
  createdAt: string
  createdAtDisplay: string
  finishedAt: string | null
  finishedAtDisplay: string | null
  stopRequested: boolean
  total: number
  counts: Record<ImportItemStatus, number> & { importedAccounts: number }
}

export interface AccountImportTaskItem {
  index: number
  provider: string
  status: ImportItemStatus
  accountIds: string[]
  message: string | null
}

export interface AccountImportTaskDetail extends AccountImportTask {
  items: AccountImportTaskItem[]
}

export interface AccountOAuthCompleteResponse {
  accountId: string
}

export interface AccountUpdateResponse {
  accountId: string
  configRevision: number
}

export interface AccountBatchUpdateResponse {
  accountIds: string[]
  configRevision: number
}

export interface AccountDeletionResponse {
  deletedCount: number
  accountIds: string[]
}

export interface AccountOAuthStartResponse {
  flowId: string
  authorizationUrl: string
  expiresAt: string
}

// 请求参数类型：仅定义 API 边界的形状，调用方不依赖显式声明。
interface AccountListParams {
  page: number
  pageSize: number
  search?: string
  provider?: string
  status?: string
  groupId?: string
  sortBy?: string
  sortDirection?: string
}

interface AccountIdParam {
  accountId: string
}

interface AccountResetCreditConsumeParam extends AccountIdParam {
  creditId?: string
  redeemRequestId: string
}

interface AccountUpdateParam {
  connection?: { baseUrl?: string, transport: ApiKeyConfiguration['transport'], apiKey?: string }
  outboundProxyUrl?: string
  outboundProxyId?: string
  accountId: string
  notes?: string
  enabled: boolean
  concurrencyLimit: number | null
  weight: number
  modelAccess?: AccountModelAccess
  groupIds: string[]
}

interface AccountBatchUpdateParam {
  outboundProxyUrl?: string
  outboundProxyId?: string
  accountIds: string[]
  enabled?: boolean
  concurrencyLimit?: number | null
  weight?: number
  modelAccess?: AccountModelAccess
  groupIds?: string[]
}

interface AccountDeleteParams {
  provider: string
  accountIds: string[]
}

export interface AccountImportSettings {
  notes?: string
  enabled: boolean
  concurrencyLimit: number | null
  weight: number
  modelAccess?: AccountModelAccess
  groupIds: string[]
}

export interface AccountImportItem {
  outboundProxyId?: string
  settings?: AccountImportSettings
  provider: string
  data: unknown
}

interface AccountImportTaskIdParam {
  taskId: string
}

interface CreateAccountImportTaskParam {
  submissionId: string
  items: AccountImportItem[]
}

interface AccountOAuthStartParam {
  outboundProxyUrl?: string
  outboundProxyId?: string
  provider: string
  name: string
  accountId?: string
}

interface AccountOAuthCompleteParam {
  settings?: AccountImportSettings
  provider: string
  flowId: string
  callbackUrl: string
}

interface AccountExportParam {
  accountIds: string
  confirm: string
}

export function getAccounts(data: AccountListParams, options: RequestOptions = {}) {
  return request<AccountListResponse>({
    url: '/api/admin/accounts',
    method: 'GET',
    params: data,
    ...options,
  })
}

export function exportAccounts(data: AccountExportParam) {
  return request<{ exportedAt: string, fileName: string, documents: unknown[] }>({
    url: '/api/admin/accounts/export',
    method: 'GET',
    params: data,
  })
}

export function refreshAccount(data: AccountIdParam) {
  return request<AccountRefreshResponse>({
    url: '/api/admin/accounts/refresh',
    method: 'POST',
    data,
  })
}

export function recoverAccount(data: AccountIdParam) {
  return request<AccountRefreshResponse>({
    url: '/api/admin/accounts/recover',
    method: 'POST',
    data,
  })
}

export function getAccountPersonalInfo(data: AccountIdParam, options: RequestOptions = {}) {
  return request<AccountPersonalInfoResponse>({
    url: '/api/admin/accounts/personal-info',
    method: 'GET',
    params: data,
    ...options,
  })
}

export function getAccountQuotaForecast(data: AccountIdParam, options: RequestOptions = {}) {
  return request<AccountQuotaForecastResponse>({
    url: '/api/admin/accounts/quota-forecast',
    method: 'GET',
    params: data,
    ...options,
  })
}

export function accountProfileAvatarUrl(accountId: string, sourceUrl: string) {
  const params = new URLSearchParams({
    accountId,
    version: stableAvatarVersion(sourceUrl),
  })
  return `/api/admin/accounts/profile-avatar?${params.toString()}`
}

function stableAvatarVersion(value: string) {
  let hash = 0
  for (const character of value)
    hash = (hash * 33 + (character.codePointAt(0) ?? 0)) % 2_147_483_647
  return hash.toString(36)
}

export function refreshAccountQuota(data: AccountIdParam, options: RequestOptions = {}) {
  return request<AccountQuotaResponse>({
    url: '/api/admin/accounts/quota/refresh',
    method: 'POST',
    data,
    ...options,
  })
}

export function getAccountResetCredits(data: AccountIdParam, options: RequestOptions = {}) {
  return request<AccountResetCreditsResponse>({
    url: '/api/admin/accounts/reset-credits',
    method: 'GET',
    params: data,
    ...options,
  })
}

export function consumeAccountResetCredit(data: AccountResetCreditConsumeParam, options: RequestOptions = {}) {
  return request<AccountResetCreditResultResponse>({
    url: '/api/admin/accounts/reset-credits',
    method: 'POST',
    data,
    ...options,
  })
}

export function getAccountModels(data: AccountIdParam, options: RequestOptions = {}) {
  return request<AccountModelsResponse>({
    url: '/api/admin/accounts/models',
    method: 'GET',
    params: data,
    ...options,
  })
}

export type AccountConnectionTestReasoningEffort
  = 'none' | 'minimal' | 'low' | 'medium' | 'high' | 'xhigh'

export interface AccountConnectionTestParam {
  accountIds: string[]
  modelId: string
  // 省略或 null 时服务端使用默认问题 `Reply with exactly OK.`
  inputText?: string | null
  // 省略或 null 表示不发送 reasoning 字段
  reasoningEffort?: AccountConnectionTestReasoningEffort | null
}

export type AccountConnectionTestFailureSource = 'gateway' | 'provider' | 'upstream'
export type AccountConnectionTestSendState = 'not_sent' | 'sent' | 'ambiguous'

export interface AccountConnectionTestRequestPayload {
  input?: Array<{ content?: Array<{ type?: string, text?: string }> }>
}

interface AccountConnectionTestEventMeta {
  // 批量接口的每条事件都带 accountId，用来分发到对应账号
  accountId?: string
  occurredAtDisplay?: string
  timeDisplay?: string
}

export type AccountConnectionTestEvent = AccountConnectionTestEventMeta & (
  | { type: 'test_start', text?: string, model?: string }
  | { type: 'request', payload?: AccountConnectionTestRequestPayload }
  | { type: 'status', text?: string }
  | { type: 'content', text?: string }
  | { type: 'test_complete', success: boolean, error?: string }
  | {
    type: 'error'
    source?: AccountConnectionTestFailureSource
    gatewayErrorCode?: string
    sendState?: AccountConnectionTestSendState | null
    error?: string
    providerErrorCode?: string | null
    providerErrorType?: string | null
    upstreamStatus?: number | null
    upstreamContentType?: string | null
    upstreamBody?: string | null
  }
  | { type: 'batch_complete' }
)

export type AccountConnectionTestFailureEvent = Extract<AccountConnectionTestEvent, { type: 'error' }>

const CONNECTION_TEST_EVENT_TYPES = new Set<string>([
  'test_start',
  'request',
  'status',
  'content',
  'test_complete',
  'error',
  'batch_complete',
])

// 与 request.ts 的会话失效业务码保持一致
const SESSION_REQUIRED = 40101

/**
 * 批量连接测试：EventSource 不支持 POST，这里用 fetch 读取 SSE 正文，
 * 按空行切分事件后把 `data:` 的 JSON 逐条回调。未知类型的事件直接跳过
 */
export async function streamAccountConnectionTests(
  data: AccountConnectionTestParam,
  options: { signal?: AbortSignal, onEvent: (event: AccountConnectionTestEvent) => void },
) {
  const response = await fetch(`${API_BASE_URL}/api/admin/accounts/connection-test`, {
    method: 'POST',
    credentials: 'include',
    headers: { 'Content-Type': 'application/json', 'Accept': 'text/event-stream' },
    body: JSON.stringify(data),
    signal: options.signal,
  })
  if (!response.ok)
    throw await connectionTestResponseError(response)
  if (!response.body)
    throw new ApiError('当前浏览器不支持流式响应', response.status, undefined, requestIdOf(response), 'http')

  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''
  const flush = (final: boolean) => {
    buffer = buffer.replace(/\r\n?/g, '\n')
    const blocks = buffer.split('\n\n')
    // 最后一段可能还没收完，留到下次再解析
    buffer = final ? '' : blocks.pop() ?? ''
    for (const block of blocks) {
      const event = parseConnectionTestBlock(block)
      if (event)
        options.onEvent(event)
    }
  }
  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done)
        break
      buffer += decoder.decode(value, { stream: true })
      flush(false)
    }
    buffer += decoder.decode()
    flush(true)
  }
  finally {
    reader.releaseLock()
  }
}

function parseConnectionTestBlock(block: string): AccountConnectionTestEvent | null {
  const data = block
    .split('\n')
    .filter(line => line.startsWith('data:'))
    .map(line => line.slice(5).replace(/^ /, ''))
    .join('\n')
  if (!data.trim())
    return null
  let value: unknown
  try {
    value = JSON.parse(data)
  }
  catch {
    throw new ApiError('测试响应解析失败', 200, undefined, undefined, 'http')
  }
  if (!value || typeof value !== 'object' || !('type' in value) || typeof value.type !== 'string')
    throw new ApiError('测试响应解析失败', 200, undefined, undefined, 'http')
  if (!CONNECTION_TEST_EVENT_TYPES.has(value.type))
    return null
  return value as AccountConnectionTestEvent
}

function requestIdOf(response: Response) {
  return response.headers.get('x-request-id') || undefined
}

async function connectionTestResponseError(response: Response) {
  let code: number | undefined
  let message = ''
  try {
    const body: unknown = await response.json()
    if (body && typeof body === 'object') {
      const record = body as Record<string, unknown>
      if (typeof record.code === 'number')
        code = record.code
      if (typeof record.message === 'string')
        message = record.message.trim()
    }
  }
  catch {
    // 错误体不是 JSON 时使用下面的兜底文案
  }
  if (response.status === 401 && (code === undefined || code === SESSION_REQUIRED)) {
    // 流式请求绕过了 axios 拦截器，这里只提示；调用方随后的静默刷新会走统一的会话失效跳转
    toast.error('登录已失效')
    message = '登录已失效'
  }
  return new ApiError(
    message || `请求失败（HTTP ${response.status}）`,
    response.status,
    code,
    requestIdOf(response),
    'api',
  )
}

export function getAccountModelCatalog(data: AccountIdParam, options: RequestOptions = {}) {
  return request<AccountModelCatalogResponse>({
    url: '/api/admin/accounts/models/catalog',
    method: 'GET',
    params: data,
    ...options,
  })
}

export function refreshAccountModels(data: AccountIdParam, options: RequestOptions = {}) {
  return request<AccountModelsResponse>({
    url: '/api/admin/accounts/models/refresh',
    method: 'POST',
    data,
    ...options,
  })
}

export function importAccounts(data: AccountImportItem, options: RequestOptions = {}) {
  return request<AccountImportResponse>({
    url: '/api/admin/accounts/import',
    method: 'POST',
    ...providerInputBody(data),
    ...options,
  })
}

export function createAccountImportTask(data: CreateAccountImportTaskParam) {
  return request<AccountImportTask>({
    url: '/api/admin/accounts/import-tasks',
    method: 'POST',
    ...providerInputBody(data),
  })
}

export function getAccountImportTasks(options: RequestOptions = {}) {
  return request<{ items: AccountImportTask[] }>({
    url: '/api/admin/accounts/import-tasks',
    method: 'GET',
    ...options,
  })
}

export function getAccountImportTask(data: AccountImportTaskIdParam, options: RequestOptions = {}) {
  return request<AccountImportTaskDetail>({
    url: '/api/admin/accounts/import-tasks/detail',
    method: 'GET',
    params: data,
    ...options,
  })
}

export function stopAccountImportTask(data: AccountImportTaskIdParam) {
  return request<AccountImportTaskDetail>({
    url: '/api/admin/accounts/import-tasks/stop',
    method: 'POST',
    data,
  })
}

export function updateAccount(data: AccountUpdateParam) {
  return request<AccountUpdateResponse>({
    url: '/api/admin/accounts/update',
    method: 'POST',
    data,
  })
}

export function batchUpdateAccounts(data: AccountBatchUpdateParam) {
  return request<AccountBatchUpdateResponse>({
    url: '/api/admin/accounts/batch-update',
    method: 'POST',
    data,
  })
}

export function deleteAccounts(data: AccountDeleteParams, options: RequestOptions = {}) {
  return request<AccountDeletionResponse>({
    url: '/api/admin/accounts/delete',
    method: 'POST',
    data,
    ...options,
  })
}

export function startAccountOAuth(data: AccountOAuthStartParam, options: RequestOptions = {}) {
  return request<AccountOAuthStartResponse>({
    url: '/api/admin/accounts/oauth/start',
    method: 'POST',
    ...providerInputBody(data),
    ...options,
  })
}

function providerInputBody(data: object) {
  // Provider 文档是不透明 JSON，先编码，避免 HTTP 客户端合并配置时过滤特殊字段名。
  return { headers: { 'Content-Type': 'application/json' }, data: JSON.stringify(data) }
}

export function completeAccountOAuth(data: AccountOAuthCompleteParam, options: RequestOptions = {}) {
  return request<AccountOAuthCompleteResponse>({
    url: '/api/admin/accounts/oauth/complete',
    method: 'POST',
    data,
    ...options,
  })
}

export interface ApiKeyConfiguration {
  base_url: string
  transport: 'http' | 'prefer_websocket'
}

export function getAccountDetail(data: AccountIdParam, options: RequestOptions = {}) {
  return request<{ account: Account, credentialConfiguration?: Record<string, unknown> }>({
    url: '/api/admin/accounts/detail',
    method: 'GET',
    params: data,
    ...options,
  })
}
