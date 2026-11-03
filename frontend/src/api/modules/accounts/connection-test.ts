import { notification } from '@/utils/notification'
import { API_BASE_URL } from '../../constants'
import { ApiError } from '../../request'

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

// 与 request 的会话失效业务码保持一致
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
    notification.error({ message: '登录已失效' })
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
