import type {
  Account,
  AccountConnectionTestEvent,
  AccountConnectionTestFailureEvent,
  AccountConnectionTestFailureSource,
  AccountConnectionTestReasoningEffort,
  AccountConnectionTestRequestPayload,
  AccountModelsResponse,
} from '@/api'
import { toast } from '@codex-proxy/ui'

import { CheckCircle2, Clock3, Wifi, XCircle } from '@lucide/vue'
import { clamp } from 'es-toolkit'
import { computed, onBeforeUnmount, ref, shallowRef, watch } from 'vue'
import { getAccountModels, refreshAccountModels, streamAccountConnectionTests } from '@/api'
import { ApiError } from '@/api/error'
import { useIdSet } from '@/composables/useIdSet'
import { useRequestState } from '@/composables/useRequestState'
import { errorMessage, withMinimumDuration } from '@/utils/operation'

interface ConnectionTestRun {
  accountId: string
  controller: AbortController
}

type ConnectionTestStatus = 'idle' | 'running' | 'success' | 'error'
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

interface ConnectionTestTimestamp { occurredAtDisplay?: string, timeDisplay?: string }

export const CONNECTION_TEST_FAILURE_TEXT: Record<string, string> = {
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

// 「默认」对应空字符串，提交时不传 reasoningEffort
export const CONNECTION_TEST_REASONING_OPTIONS: Array<{ label: string, value: '' | AccountConnectionTestReasoningEffort }> = [
  { label: '默认', value: '' },
  { label: 'none', value: 'none' },
  { label: 'minimal', value: 'minimal' },
  { label: 'low', value: 'low' },
  { label: 'medium', value: 'medium' },
  { label: 'high', value: 'high' },
  { label: 'xhigh', value: 'xhigh' },
]

export const CONNECTION_TEST_INPUT_MAX_LENGTH = 8000

export function connectionTestReasoningEffort(value: string): AccountConnectionTestReasoningEffort | null {
  return CONNECTION_TEST_REASONING_OPTIONS.some(option => option.value && option.value === value)
    ? value as AccountConnectionTestReasoningEffort
    : null
}

const CONNECTION_TEST_SOURCE_LABEL: Record<AccountConnectionTestFailureSource, string> = {
  gateway: '网关校验',
  provider: 'Provider 本地准备',
  upstream: '上游响应',
}

export function connectionTestFailureText(event: AccountConnectionTestFailureEvent) {
  return event.gatewayErrorCode
    ? CONNECTION_TEST_FAILURE_TEXT[event.gatewayErrorCode] || '未分类错误'
    : '测试连接失败'
}

function connectionTestFailureLabel(event: AccountConnectionTestFailureEvent) {
  return event.source ? CONNECTION_TEST_SOURCE_LABEL[event.source] || '测试失败' : '测试失败'
}

function connectionTestFailureDiagnostics(event: AccountConnectionTestFailureEvent) {
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
  const modelsRequest = useRequestState()
  const modelsRequestMode = shallowRef<'load' | 'refresh'>('load')
  const loadingConnectionTestModels = computed(() => modelsRequest.loading.value && modelsRequestMode.value === 'load')
  const refreshingConnectionTestModels = computed(() => modelsRequest.loading.value && modelsRequestMode.value === 'refresh')
  const connectionTestSelectedModel = shallowRef('')
  const connectionTestModelOptions = ref<ConnectionTestModelOption[]>([])
  // 留空时不传 inputText，服务端使用默认问题，行为与原来一致
  const connectionTestInputText = shallowRef('')
  // 空字符串表示「默认」，提交时不传 reasoningEffort
  const connectionTestReasoning = shallowRef('')

  let connectionTestStartedAtMs = 0
  let connectionTestRun: ConnectionTestRun | undefined

  const connectionTestStatusView = computed(() => {
    if (connectionTestStatus.value === 'running') {
      return {
        label: '正在测试',
        description: '正在向所选模型发送请求并接收流式响应',
        icon: Clock3,
        badge: 'bg-cp-info-container text-cp-info-on-container',
        iconClass: 'text-cp-info',
      }
    }
    if (connectionTestStatus.value === 'success') {
      return {
        label: '连接正常',
        description: '请求已完成，可在下方查看模型、耗时和事件轨迹',
        icon: CheckCircle2,
        badge: 'bg-cp-success-container text-cp-success-on-container',
        iconClass: 'text-cp-success',
      }
    }
    if (connectionTestStatus.value === 'error') {
      return {
        label: '测试失败',
        description: '请求未完成，请在下方查看失败来源与原始诊断',
        icon: XCircle,
        badge: 'bg-cp-error-container text-cp-error-on-container',
        iconClass: 'text-cp-error',
      }
    }
    return {
      label: '准备测试',
      description: '选择模型后，点击“开始测试”发送真实请求',
      icon: Wifi,
      badge: 'bg-cp-fill-quaternary text-cp-text-secondary',
      iconClass: 'text-cp-text-quaternary',
    }
  })

  function openConnectionTest(account: Account) {
    abortConnectionTest()
    testingAccount.value = account
    connectionTestSelectedModel.value = ''
    connectionTestModelOptions.value = []
    showConnectionTestModal.value = true
    resetConnectionTest()
    void loadConnectionTestModels(account)
  }

  function resetConnectionTest() {
    connectionTestStatus.value = 'idle'
    connectionTestModel.value = ''
    connectionTestContent.value = ''
    connectionTestLogs.value = []
    connectionTestError.value = ''
    connectionTestStartedAt.value = ''
    connectionTestFinishedAt.value = ''
    connectionTestDurationMs.value = null
    connectionTestStartedAtMs = 0
  }

  function formatConnectionTestDetail(value: unknown) {
    if (value === undefined || value === null || value === '')
      return ''
    if (typeof value === 'string')
      return value
    return JSON.stringify(value, null, 2)
  }

  function connectionTestRequestText(payload?: AccountConnectionTestRequestPayload) {
    const texts = (payload?.input ?? [])
      .flatMap(item => item.content ?? [])
      .filter(item => item.type === 'input_text' && item.text)
      .map(item => item.text)
    return texts.join('\n')
  }

  function connectionTestLogItem(
    key: string,
    text: string,
    tone: ConnectionTestLogTone = 'normal',
    detail?: unknown,
    event?: ConnectionTestTimestamp,
  ): ConnectionTestLog {
    return {
      key,
      time: event?.timeDisplay ?? '',
      text,
      tone,
      detail: formatConnectionTestDetail(detail),
    }
  }

  function appendConnectionTestLog(
    text: string,
    tone: ConnectionTestLogTone = 'normal',
    detail?: unknown,
    event?: ConnectionTestTimestamp,
  ) {
    connectionTestLogs.value = [
      ...connectionTestLogs.value,
      connectionTestLogItem(`${Date.now()}-${connectionTestLogs.value.length}`, text, tone, detail, event),
    ]
  }

  function setConnectionTestLog(
    key: string,
    text: string,
    tone: ConnectionTestLogTone = 'normal',
    detail?: unknown,
    event?: ConnectionTestTimestamp,
  ) {
    const index = connectionTestLogs.value.findIndex(item => item.key === key)
    const next = connectionTestLogItem(key, text, tone, detail, event)
    if (index === -1) {
      connectionTestLogs.value = [...connectionTestLogs.value, next]
      return
    }
    connectionTestLogs.value = connectionTestLogs.value.map((item, itemIndex) =>
      itemIndex === index ? { ...next, time: item.time } : item,
    )
  }

  function finishConnectionTest(status: 'success' | 'error', event?: ConnectionTestTimestamp) {
    connectionTestStatus.value = status
    connectionTestFinishedAt.value = event?.occurredAtDisplay ?? ''
    connectionTestDurationMs.value = clamp(
      performance.now() - connectionTestStartedAtMs,
      0,
      Number.POSITIVE_INFINITY,
    )
  }

  function clearConnectionTestRun() {
    const run = connectionTestRun
    connectionTestRun = undefined
    if (run) {
      testingConnections.remove(run.accountId)
      // 已收到结束事件或主动关闭时中断请求，服务端随之取消未完成的探测
      run.controller.abort()
    }
  }

  function recordConnectionTestFailure(
    key: string,
    label: string,
    message: string,
    detail?: unknown,
    event?: ConnectionTestTimestamp,
  ) {
    connectionTestError.value = message
    setConnectionTestLog(key, `${label}：${message}`, 'danger', detail, event)
    finishConnectionTest('error', event)
  }

  function handleConnectionTestEvent(event: AccountConnectionTestEvent) {
    if (event.type === 'test_start') {
      connectionTestStartedAt.value = event.occurredAtDisplay ?? ''
      connectionTestModel.value = event.model || connectionTestModel.value
      appendConnectionTestLog(`开始测试 ${connectionTestModel.value || '未选择模型'}`, 'info', undefined, event)
      return
    }
    if (event.type === 'request') {
      setConnectionTestLog('request', '发起请求', 'info', connectionTestRequestText(event.payload), event)
      return
    }
    if (event.type === 'status' && event.text) {
      appendConnectionTestLog(event.text, 'info', undefined, event)
      return
    }
    if (event.type === 'content' && event.text) {
      connectionTestContent.value += event.text
      setConnectionTestLog('response', '接收响应内容', 'success', connectionTestContent.value, event)
      return
    }
    if (event.type === 'test_complete') {
      if (event.success) {
        if (!connectionTestContent.value) {
          setConnectionTestLog('response', '响应完成', 'success', '上游已完成，没有返回文本内容', event)
        }
        appendConnectionTestLog('测试完成', 'success', undefined, event)
        finishConnectionTest('success', event)
      }
      else {
        recordConnectionTestFailure(
          'test-complete-failure',
          '测试失败',
          '测试连接失败',
          { error: event.error ?? null },
          event,
        )
      }
      clearConnectionTestRun()
      void options.reload()
      return
    }
    if (event.type === 'error') {
      recordConnectionTestFailure(
        `failure-${event.source || 'unknown'}`,
        connectionTestFailureLabel(event),
        connectionTestFailureText(event),
        connectionTestFailureDiagnostics(event),
        event,
      )
      clearConnectionTestRun()
      void options.reload()
    }
  }

  function abortConnectionTest() {
    clearConnectionTestRun()
  }

  async function loadConnectionTestModels(account = testingAccount.value, refresh = false) {
    if (!account?.id || !showConnectionTestModal.value || account.id !== testingAccount.value?.id)
      return
    // 初次查询与主动刷新属于同一弹窗会话，旧请求不得覆盖新账号或新一轮查询。
    const requestId = modelsRequest.start()
    modelsRequestMode.value = refresh ? 'refresh' : 'load'
    connectionTestError.value = ''
    try {
      const result = await (refresh ? refreshAccountModels : getAccountModels)(
        { accountId: account.id },
        { signal: modelsRequest.signal },
      )
      if (!modelsRequest.isCurrent(requestId))
        return
      applyConnectionTestModels(result, refresh)
      if (refresh) {
        toast.success(`已刷新 ${connectionTestModelOptions.value.length} 个上游模型`)
      }
      else if (!connectionTestSelectedModel.value) {
        connectionTestError.value = '没有可测试模型'
      }
    }
    catch (error: unknown) {
      if (!modelsRequest.isCurrent(requestId))
        return
      connectionTestError.value = errorMessage(error)
      if (!refresh) {
        connectionTestModelOptions.value = []
        connectionTestSelectedModel.value = ''
      }
    }
    finally {
      modelsRequest.finish(requestId)
    }
  }

  function applyConnectionTestModels(result: AccountModelsResponse, preserveSelection = false) {
    const previousSelection = preserveSelection ? connectionTestSelectedModel.value : ''
    connectionTestModelOptions.value = []
    for (const model of result.models ?? []) {
      connectionTestModelOptions.value.push({
        label: model.label || model.id,
        value: model.id,
      })
    }
    connectionTestSelectedModel.value = connectionTestModelOptions.value.some(
      model => model.value === previousSelection,
    )
      ? previousSelection
      : connectionTestModelOptions.value[0]?.value || ''
  }

  async function handleRefreshConnectionTestModels(account = testingAccount.value) {
    if (modelsRequest.loading.value)
      return
    await loadConnectionTestModels(account, true)
  }

  async function handleTestConnection(account = testingAccount.value) {
    if (!account?.id)
      return
    if (!connectionTestSelectedModel.value) {
      connectionTestError.value = '请先选择测试模型'
      return
    }
    const inputText = connectionTestInputText.value.trim()
    if (inputText.length > CONNECTION_TEST_INPUT_MAX_LENGTH) {
      connectionTestError.value = `测试问题最多 ${CONNECTION_TEST_INPUT_MAX_LENGTH} 个字符`
      return
    }
    if (testingConnections.has(account.id))
      return
    abortConnectionTest()
    connectionTestStatus.value = 'running'
    connectionTestModel.value = ''
    connectionTestContent.value = ''
    connectionTestLogs.value = []
    connectionTestError.value = ''
    connectionTestDurationMs.value = null
    connectionTestModel.value = connectionTestSelectedModel.value
    connectionTestStartedAtMs = performance.now()
    connectionTestStartedAt.value = ''
    connectionTestFinishedAt.value = ''
    appendConnectionTestLog('准备发送测试请求', 'info')
    testingConnections.add(account.id)
    const run: ConnectionTestRun = { accountId: account.id, controller: new AbortController() }
    connectionTestRun = run
    try {
      await withMinimumDuration(() => streamAccountConnectionTests(
        {
          accountIds: [account.id],
          modelId: connectionTestSelectedModel.value,
          inputText: inputText || null,
          reasoningEffort: connectionTestReasoningEffort(connectionTestReasoning.value),
        },
        {
          signal: run.controller.signal,
          onEvent: (event) => {
            // 旧一轮测试的残留事件不得写入当前弹窗
            if (connectionTestRun !== run || event.type === 'batch_complete')
              return
            handleConnectionTestEvent(event)
          },
        },
      ))
      if (connectionTestRun === run && connectionTestStatus.value === 'running') {
        recordConnectionTestFailure('failure', '测试失败', '测试连接未返回完成事件')
      }
    }
    catch (error: unknown) {
      // 主动中断（关闭弹窗或已收到结束事件）不算失败
      if (run.controller.signal.aborted || connectionTestRun !== run)
        return
      recordConnectionTestFailure('failure', '测试失败', errorMessage(error, '测试连接失败'))
      // 401 等会话问题交给静默刷新里的统一会话失效处理
      if (error instanceof ApiError && error.status === 401)
        void options.reload()
    }
    finally {
      if (connectionTestRun === run)
        clearConnectionTestRun()
    }
  }

  watch([showConnectionTestModal, () => testingAccount.value?.id], ([open]) => {
    modelsRequest.invalidate({ resetLoading: open })
    if (!open) {
      abortConnectionTest()
    }
  }, { flush: 'sync' })

  onBeforeUnmount(() => {
    abortConnectionTest()
  })

  return {
    showConnectionTestModal,
    testingAccount,
    connectionTestStatus,
    connectionTestModel,
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
    connectionTestInputText,
    connectionTestReasoning,
    connectionTestStatusView,
    openConnectionTest,
    handleRefreshConnectionTestModels,
    handleTestConnection,
  }
}
