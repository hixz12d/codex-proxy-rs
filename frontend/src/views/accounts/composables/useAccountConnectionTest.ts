import type { Account, AccountModelsResponse } from '@/api'
import { CheckCircle2, CircleStop, Clock3, Wifi, XCircle } from '@lucide/vue'

import { computed, onBeforeUnmount, ref, shallowRef, watch } from 'vue'
import { getAccountModels, refreshAccountModels } from '@/api'
import { readConnectionTestEvents } from '@/api/connection-test-stream'
import request from '@/api/request'
import { toast } from '@/components/base/BaseToast'
import { useIdSet } from '@/composables/useIdSet'
import { errorMessage } from '@/utils/async'
import { formatDateTime, formatTime } from '@/utils/date'

interface ConnectionTestRun {
  accountId: string
  controller: AbortController
}

type ConnectionTestStatus = 'idle' | 'running' | 'success' | 'error' | 'cancelled'
type ConnectionTestLogTone = 'normal' | 'info' | 'success' | 'danger'

interface ConnectionTestModelOption {
  label: string
  value: string
}

interface ConnectionTestLog {
  key: string
  time: string
  text: string
  tone: ConnectionTestLogTone
  detail: string
}

interface ConnectionTestRequestPayload {
  input?: Array<{ content?: Array<{ type?: string, text?: string }> }>
  reasoning?: { effort?: string } | null
}

interface ConnectionTestStartEvent {
  type: 'test_start'
  text?: string
  model?: string
}

interface ConnectionTestRequestEvent {
  type: 'request'
  payload?: ConnectionTestRequestPayload
}

interface ConnectionTestStatusEvent {
  type: 'status'
  text?: string
}

interface ConnectionTestContentEvent {
  type: 'content'
  text?: string
}

interface ConnectionTestCompleteEvent {
  type: 'test_complete'
  success: boolean
  error?: string
}

type ConnectionTestFailureSource = 'gateway' | 'provider' | 'upstream'
type ConnectionTestSendState = 'not_sent' | 'sent' | 'ambiguous'

interface ConnectionTestFailureEvent {
  type: 'error'
  source?: ConnectionTestFailureSource
  gatewayErrorCode?: string
  sendState?: ConnectionTestSendState | null
  error?: string
  providerErrorCode?: string | null
  providerErrorType?: string | null
  upstreamStatus?: number | null
  upstreamContentType?: string | null
  upstreamBody?: string | null
}

type ConnectionTestEvent
  = | ConnectionTestStartEvent
    | ConnectionTestRequestEvent
    | ConnectionTestStatusEvent
    | ConnectionTestContentEvent
    | ConnectionTestCompleteEvent
    | ConnectionTestFailureEvent

const CONNECTION_TEST_EVENT_TYPES = new Set<ConnectionTestEvent['type']>([
  'test_start',
  'request',
  'status',
  'content',
  'test_complete',
  'error',
])

const CONNECTION_TEST_FAILURE_TEXT: Record<string, string> = {
  invalid_request: '测试请求不合法',
  unsupported: '当前 Provider 不支持连接测试',
  unauthorized: '账号凭据无效',
  policy_denied: '测试请求被网关策略拒绝',
  model_not_found: '测试模型不存在',
  no_available_provider: '指定账号当前不可用于连接测试',
  account_capacity_unavailable: '指定账号当前没有可用容量',
  provider_infrastructure_unavailable: 'Provider 本地基础设施暂不可用',
  rate_limited: '上游请求过于频繁',
  upstream_unavailable: '上游服务暂不可用',
  timeout: '上游请求超时',
  cancelled: '测试请求已取消',
  internal_error: '网关内部错误',
}

const CONNECTION_TEST_SOURCE_LABEL: Record<ConnectionTestFailureSource, string> = {
  gateway: '网关校验',
  provider: 'Provider 本地准备',
  upstream: '上游响应',
}

function parseConnectionTestEvent(raw: string): ConnectionTestEvent | null {
  const value: unknown = JSON.parse(raw)
  if (!value || typeof value !== 'object' || !('type' in value) || typeof value.type !== 'string')
    throw new TypeError('invalid connection-test event')
  if (!CONNECTION_TEST_EVENT_TYPES.has(value.type as ConnectionTestEvent['type']))
    return null
  return value as ConnectionTestEvent
}

function connectionTestFailureText(event: ConnectionTestFailureEvent) {
  return event.gatewayErrorCode
    ? CONNECTION_TEST_FAILURE_TEXT[event.gatewayErrorCode] || '未分类错误'
    : '测试连接失败'
}

function connectionTestFailureLabel(event: ConnectionTestFailureEvent) {
  return event.source ? CONNECTION_TEST_SOURCE_LABEL[event.source] || '测试失败' : '测试失败'
}

function connectionTestFailureDiagnostics(event: ConnectionTestFailureEvent) {
  return {
    error: event.error ?? null,
    gatewayErrorCode: event.gatewayErrorCode ?? null,
    sendState: event.sendState ?? null,
    upstreamStatus: event.upstreamStatus ?? null,
    providerErrorCode: event.providerErrorCode ?? null,
    providerErrorType: event.providerErrorType ?? null,
    upstreamContentType: event.upstreamContentType ?? null,
    upstreamBody: event.upstreamBody ?? null,
  }
}

export function useAccountConnectionTest(options: { reload: () => Promise<unknown> }) {
  const showConnectionTestModal = shallowRef(false)
  const testingAccount = shallowRef<Account | null>(null)
  const connectionTestStatus = shallowRef<ConnectionTestStatus>('idle')
  const connectionTestModel = shallowRef('')
  const connectionTestContent = shallowRef('')
  const connectionTestLogs = ref<ConnectionTestLog[]>([])
  const connectionTestError = shallowRef('')
  const connectionTestStartedAt = shallowRef('')
  const connectionTestFinishedAt = shallowRef('')
  const connectionTestDurationMs = shallowRef<number | null>(null)
  const testingConnections = useIdSet<string>()
  const loadingConnectionTestModels = shallowRef(false)
  const refreshingConnectionTestModels = shallowRef(false)
  const connectionTestSelectedModel = shallowRef('')
  const connectionTestModelOptions = ref<ConnectionTestModelOption[]>([])
  const connectionTestMode = shallowRef<'quick' | 'manual'>('quick')
  const connectionTestQuestion = shallowRef('')
  const connectionTestReasoningEffort = shallowRef('default')
  let startedAtMs = 0
  let run: ConnectionTestRun | undefined
  let modelLoadGeneration = 0

  const connectionTestStatusView = computed(() => {
    const status = connectionTestStatus.value
    if (status === 'running') {
      return {
        label: '正在测试',
        description: '等待上游回答',
        icon: Clock3,
        badge: 'bg-cp-info-container text-cp-info-on-container',
        iconClass: 'text-cp-info',
      }
    }
    if (status === 'success') {
      return {
        label: '回答接收完成',
        description: '请求已完成',
        icon: CheckCircle2,
        badge: 'bg-cp-success-container text-cp-success-on-container',
        iconClass: 'text-cp-success',
      }
    }
    if (status === 'error') {
      return {
        label: '测试失败',
        description: '请求未完成',
        icon: XCircle,
        badge: 'bg-cp-error-container text-cp-error-on-container',
        iconClass: 'text-cp-error',
      }
    }
    if (status === 'cancelled') {
      return {
        label: '已停止',
        description: '测试连接已关闭，上游可能已产生用量',
        icon: CircleStop,
        badge: 'bg-cp-fill-quaternary text-cp-text-secondary',
        iconClass: 'text-cp-text-quaternary',
      }
    }
    return {
      label: '准备测试',
      description: '尚未发送请求',
      icon: Wifi,
      badge: 'bg-cp-fill-quaternary text-cp-text-secondary',
      iconClass: 'text-cp-text-quaternary',
    }
  })

  function resetConnectionTest() {
    connectionTestStatus.value = 'idle'
    connectionTestModel.value = ''
    connectionTestContent.value = ''
    connectionTestLogs.value = []
    connectionTestError.value = ''
    connectionTestStartedAt.value = ''
    connectionTestFinishedAt.value = ''
    connectionTestDurationMs.value = null
  }

  function openConnectionTest(account: Account) {
    abortConnectionTest()
    testingAccount.value = account
    connectionTestSelectedModel.value = ''
    connectionTestReasoningEffort.value = 'default'
    connectionTestModelOptions.value = []
    showConnectionTestModal.value = true
    resetConnectionTest()
    void loadConnectionTestModels(account)
  }

  function appendLog(text: string, tone: ConnectionTestLogTone = 'info', detail = '') {
    connectionTestLogs.value.push({
      key: `${Date.now()}-${connectionTestLogs.value.length}`,
      time: formatTime(),
      text,
      tone,
      detail,
    })
  }

  function finish(status: ConnectionTestStatus) {
    connectionTestStatus.value = status
    connectionTestFinishedAt.value = formatDateTime()
    connectionTestDurationMs.value = Math.max(0, Date.now() - startedAtMs)
  }

  function fail(message: string, detail = '') {
    connectionTestError.value = message
    appendLog(message, 'danger', detail)
    finish('error')
  }

  function abortConnectionTest() {
    if (!run)
      return
    const previous = run
    run = undefined
    previous.controller.abort()
    testingConnections.remove(previous.accountId)
    if (connectionTestStatus.value === 'running') {
      appendLog('测试已停止', 'normal')
      finish('cancelled')
    }
  }

  function handleEvent(event: ConnectionTestEvent): boolean {
    if (event.type === 'test_start') {
      connectionTestModel.value = event.model || connectionTestModel.value
      appendLog(`开始测试 ${connectionTestModel.value}`)
    }
    else if (event.type === 'request') {
      const text = (event.payload?.input ?? [])
        .flatMap(item => item.content ?? [])
        .filter(item => item.type === 'input_text' && item.text)
        .map(item => item.text)
        .join('\n')
      appendLog('发送测试问题', 'info', text)
      appendLog(`思考强度：${event.payload?.reasoning?.effort || '上游默认'}`)
    }
    else if (event.type === 'status' && event.text) {
      appendLog(event.text)
    }
    else if (event.type === 'content' && event.text) {
      if (connectionTestContent.value.length + event.text.length > 1_048_576)
        throw new Error('回答内容超出展示上限')
      connectionTestContent.value += event.text
    }
    else if (event.type === 'test_complete') {
      if (event.success === true) {
        appendLog('回答接收完成', 'success')
        finish('success')
      }
      else {
        fail('测试连接失败', event.error || '')
      }
      return false
    }
    else if (event.type === 'error') {
      fail(
        `${connectionTestFailureLabel(event)}：${connectionTestFailureText(event)}`,
        JSON.stringify(connectionTestFailureDiagnostics(event), null, 2),
      )
      return false
    }
    return true
  }

  function applyModels(result: AccountModelsResponse, preserveSelection: boolean) {
    const previous = preserveSelection ? connectionTestSelectedModel.value : ''
    connectionTestModelOptions.value = (result.models ?? []).map(model => ({
      label: model.label || model.id,
      value: model.id,
    }))
    connectionTestSelectedModel.value = connectionTestModelOptions.value.some(model => model.value === previous)
      ? previous
      : connectionTestModelOptions.value[0]?.value || ''
    if (!connectionTestSelectedModel.value)
      connectionTestError.value = '没有可测试模型'
  }

  async function loadConnectionTestModels(account: Account, refresh = false) {
    const generation = ++modelLoadGeneration
    loadingConnectionTestModels.value = !refresh
    refreshingConnectionTestModels.value = refresh
    connectionTestError.value = ''
    try {
      const result = await (refresh ? refreshAccountModels : getAccountModels)({ accountId: account.id })
      if (generation !== modelLoadGeneration || testingAccount.value?.id !== account.id)
        return
      applyModels(result, refresh)
      if (refresh)
        toast.success(`已刷新 ${connectionTestModelOptions.value.length} 个上游模型`)
    }
    catch (error: unknown) {
      if (generation !== modelLoadGeneration)
        return
      connectionTestError.value = errorMessage(error, refresh ? '刷新上游模型失败' : '加载测试模型失败')
    }
    finally {
      if (generation === modelLoadGeneration) {
        loadingConnectionTestModels.value = false
        refreshingConnectionTestModels.value = false
      }
    }
  }

  async function handleRefreshConnectionTestModels() {
    const account = testingAccount.value
    if (!account || loadingConnectionTestModels.value || refreshingConnectionTestModels.value || run)
      return
    await loadConnectionTestModels(account, true)
  }

  async function handleTestConnection() {
    const account = testingAccount.value
    if (!account || run || loadingConnectionTestModels.value || refreshingConnectionTestModels.value)
      return
    if (!connectionTestSelectedModel.value) {
      connectionTestError.value = '请先选择测试模型'
      return
    }
    const question = connectionTestMode.value === 'manual' ? connectionTestQuestion.value : undefined
    if (question !== undefined && (!question.trim() || question.length > 8000)) {
      connectionTestError.value = '检测问题不能为空，且不能超过 8000 字符'
      return
    }
    resetConnectionTest()
    const current: ConnectionTestRun = { accountId: account.id, controller: new AbortController() }
    run = current
    startedAtMs = Date.now()
    connectionTestStatus.value = 'running'
    connectionTestModel.value = connectionTestSelectedModel.value
    connectionTestStartedAt.value = formatDateTime()
    testingConnections.add(account.id)
    appendLog('准备发送测试请求')
    const timeout = setTimeout(() => {
      if (run === current) {
        fail('测试请求超时（5 分钟）')
        abortConnectionTest()
      }
    }, 300_000)
    try {
      const stream = await request<ReadableStream<Uint8Array>>({
        url: '/api/admin/accounts/connection-test',
        method: 'POST',
        adapter: 'fetch',
        responseType: 'stream',
        timeout: 0,
        signal: current.controller.signal,
        headers: { Accept: 'text/event-stream' },
        data: {
          accountId: account.id,
          modelId: connectionTestSelectedModel.value,
          inputText: question,
          reasoningEffort: connectionTestReasoningEffort.value === 'default'
            ? undefined
            : connectionTestReasoningEffort.value,
        },
      })
      if (run !== current) {
        await stream.cancel().catch(() => {})
        return
      }
      await readConnectionTestEvents(stream, (raw) => {
        if (run !== current)
          return false
        const event = parseConnectionTestEvent(raw)
        return event ? handleEvent(event) : true
      })
      if (run === current && connectionTestStatus.value === 'running')
        fail('测试连接已断开，未收到完成事件')
    }
    catch (error: unknown) {
      if (run === current)
        fail(errorMessage(error, '测试连接失败'))
    }
    finally {
      clearTimeout(timeout)
      if (run === current) {
        run = undefined
        testingConnections.remove(account.id)
        void options.reload().catch(() => {})
      }
    }
  }

  watch(connectionTestSelectedModel, () => {
    connectionTestReasoningEffort.value = 'default'
  })
  watch(showConnectionTestModal, (open) => {
    if (!open) {
      ++modelLoadGeneration
      abortConnectionTest()
    }
  }, { flush: 'sync' })
  onBeforeUnmount(() => {
    ++modelLoadGeneration
    abortConnectionTest()
  })

  return {
    showConnectionTestModal,
    testingAccount,
    connectionTestStatus,
    connectionTestModel,
    connectionTestContent,
    connectionTestLogs,
    connectionTestError,
    connectionTestStartedAt,
    connectionTestFinishedAt,
    connectionTestDurationMs,
    testingConnectionIds: testingConnections.ids,
    loadingConnectionTestModels,
    refreshingConnectionTestModels,
    connectionTestSelectedModel,
    connectionTestModelOptions,
    connectionTestStatusView,
    connectionTestMode,
    connectionTestQuestion,
    connectionTestReasoningEffort,
    openConnectionTest,
    handleRefreshConnectionTestModels,
    handleTestConnection,
    abortConnectionTest,
  }
}
