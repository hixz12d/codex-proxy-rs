import type { Ref } from 'vue'
import type { AccountConnectionTestEvent, getAccounts } from '@/api'

import { toast } from '@codex-proxy/ui'
import { useIntervalFn, useStorage } from '@vueuse/core'
import { computed, onBeforeUnmount, ref, shallowRef, watch } from 'vue'
import { getAccountModels, streamAccountConnectionTests } from '@/api'
import { ApiError } from '@/api/error'
import { errorMessage } from '@/utils/operation'
import {
  CONNECTION_TEST_INPUT_MAX_LENGTH,
  CONNECTION_TEST_REASONING_OPTIONS,
  connectionTestFailureText,
  connectionTestReasoningEffort,
} from './useAccountConnectionTest'

type AccountRow = Awaited<ReturnType<typeof getAccounts>>['items'][number]

export type CompareCardStatus = 'pending' | 'running' | 'success' | 'error' | 'stopped'

interface CompareTestParams {
  inputText: string
  modelId: string
  // 空字符串表示「默认」，提交前转换为 null
  reasoning: string
}

export interface CompareCard {
  account: AccountRow
  status: CompareCardStatus
  // 卡片上次实际发送的参数，供「重试此账号」复用
  params: CompareTestParams | null
  content: string
  contentBytes: number
  truncated: boolean
  startedAtMs: number | null
  finishedAtMs: number | null
  failureText: string
  failureDetail: string
}

interface CompareRun {
  controller: AbortController
  accountIds: Set<string>
  startedAtMs: number
  batch: boolean
  // 弹窗每次打开算一轮会话，旧会话的请求收尾时不得改写新卡片
  session: number
}

// 与后端约定一致：单次最多 200 个账号，回答最多显示约 256KB
export const COMPARE_TEST_MAX_ACCOUNTS = 200
const COMPARE_TEST_MAX_CONTENT_BYTES = 256 * 1024
const COMPARE_TEST_TICK_MS = 200

const textEncoder = new TextEncoder()

function emptyCard(account: AccountRow): CompareCard {
  return {
    account,
    status: 'pending',
    params: null,
    content: '',
    contentBytes: 0,
    truncated: false,
    startedAtMs: null,
    finishedAtMs: null,
    failureText: '',
    failureDetail: '',
  }
}

function isTerminal(status: CompareCardStatus) {
  return status === 'success' || status === 'error' || status === 'stopped'
}

// 按 UTF-8 字节截断，保证不会切出半个字符
function truncateToBytes(text: string, maxBytes: number) {
  if (maxBytes <= 0)
    return ''
  const bytes = textEncoder.encode(text)
  if (bytes.length <= maxBytes)
    return text
  // stream 模式会把截断处不完整的字符留在解码器里，不输出替换符
  return new TextDecoder().decode(bytes.slice(0, maxBytes), { stream: true })
}

export function reasoningLabel(value: string) {
  return CONNECTION_TEST_REASONING_OPTIONS.find(option => option.value === value)?.label ?? '默认'
}

export function useAccountCompareTest(options: {
  open: Ref<boolean>
  accounts: Ref<AccountRow[]>
  reload: () => unknown
}) {
  const { open } = options
  const cards = ref<CompareCard[]>([])
  const modelHints = ref<string[]>([])
  const now = shallowRef(performance.now())
  // 问题、模型、强度记在浏览器本地，下次打开自动填上
  const form = useStorage<CompareTestParams>(
    'cpr-compare-test',
    { inputText: '', modelId: '', reasoning: '' },
    localStorage,
    { mergeDefaults: true },
  )
  const runs = shallowRef<CompareRun[]>([])
  let modelsGeneration = 0
  const session = shallowRef(0)

  // 只统计当前会话，已中断的旧请求在收尾前不应锁住新弹窗
  const activeRuns = computed(() => runs.value.filter(run => run.session === session.value))
  const batchRunning = computed(() => activeRuns.value.some(run => run.batch))
  const anyRunning = computed(() => activeRuns.value.length > 0)
  const total = computed(() => cards.value.length)
  const completedCount = computed(() =>
    cards.value.filter(card => card.status === 'success' || card.status === 'error').length,
  )
  const tooMany = computed(() => cards.value.length > COMPARE_TEST_MAX_ACCOUNTS)
  const inputTooLong = computed(() => form.value.inputText.trim().length > CONNECTION_TEST_INPUT_MAX_LENGTH)
  const canStart = computed(() =>
    !anyRunning.value
    && cards.value.length > 0
    && !tooMany.value
    && !inputTooLong.value
    && Boolean(form.value.inputText.trim())
    && Boolean(form.value.modelId.trim()),
  )

  const ticker = useIntervalFn(() => {
    now.value = performance.now()
  }, COMPARE_TEST_TICK_MS, { immediate: false })

  watch(anyRunning, (running) => {
    now.value = performance.now()
    if (running)
      ticker.resume()
    else ticker.pause()
  })

  function cardElapsedMs(card: CompareCard) {
    if (card.startedAtMs === null)
      return null
    return Math.max(0, (card.finishedAtMs ?? now.value) - card.startedAtMs)
  }

  // 每次打开弹窗都按当前选中的账号重建卡片
  function prepare(accounts: AccountRow[]) {
    stopAll()
    session.value += 1
    cards.value = accounts.map(emptyCard)
    modelHints.value = []
    void loadModelHints(accounts[0])
  }

  async function loadModelHints(account: AccountRow | undefined) {
    const generation = ++modelsGeneration
    if (!account)
      return
    try {
      const result = await getAccountModels({ accountId: account.id }, { silent: true })
      if (generation !== modelsGeneration)
        return
      modelHints.value = (result.models ?? []).map(model => model.id)
      // 第一次使用时，模型默认取第一个账号的第一个模型
      if (!form.value.modelId.trim() && modelHints.value[0])
        form.value = { ...form.value, modelId: modelHints.value[0] }
    }
    catch {
      // 模型提示只是辅助，失败时仍可手填
    }
  }

  function findCard(accountId: string) {
    return cards.value.find(card => card.account.id === accountId)
  }

  function finishCard(card: CompareCard, status: CompareCardStatus, run: CompareRun) {
    card.status = status
    card.startedAtMs ??= run.startedAtMs
    card.finishedAtMs = performance.now()
  }

  function failCard(card: CompareCard, run: CompareRun, text: string, detail?: Record<string, unknown>) {
    card.failureText = text
    card.failureDetail = detail ? JSON.stringify(detail, null, 2) : ''
    finishCard(card, 'error', run)
  }

  function appendContent(card: CompareCard, text: string) {
    if (card.truncated)
      return
    const bytes = textEncoder.encode(text).length
    if (card.contentBytes + bytes <= COMPARE_TEST_MAX_CONTENT_BYTES) {
      card.content += text
      card.contentBytes += bytes
      return
    }
    card.content += truncateToBytes(text, COMPARE_TEST_MAX_CONTENT_BYTES - card.contentBytes)
    card.contentBytes = COMPARE_TEST_MAX_CONTENT_BYTES
    card.truncated = true
  }

  function handleEvent(run: CompareRun, event: AccountConnectionTestEvent) {
    if (run.session !== session.value || event.type === 'batch_complete' || !event.accountId || !run.accountIds.has(event.accountId))
      return
    const card = findCard(event.accountId)
    if (!card || isTerminal(card.status))
      return
    if (event.type === 'test_start') {
      card.status = 'running'
      card.startedAtMs = performance.now()
      return
    }
    if (event.type === 'content' && event.text) {
      appendContent(card, event.text)
      return
    }
    if (event.type === 'test_complete') {
      if (event.success)
        finishCard(card, 'success', run)
      else failCard(card, run, '测试连接失败', { error: event.error ?? null })
      return
    }
    if (event.type === 'error') {
      failCard(card, run, connectionTestFailureText(event), {
        error: event.error ?? null,
        upstreamStatus: event.upstreamStatus ?? null,
        upstreamBody: event.upstreamBody ?? null,
      })
    }
  }

  function resetCardsForRun(accountIds: Set<string>, params: CompareTestParams) {
    for (const card of cards.value) {
      if (!accountIds.has(card.account.id))
        continue
      Object.assign(card, emptyCard(card.account), { params: { ...params } })
    }
  }

  // 结束一轮：未收到结束事件的卡片按原因收尾
  function settleRun(run: CompareRun, status: 'error' | 'stopped', reason: string) {
    if (run.session !== session.value)
      return
    for (const card of cards.value) {
      if (!run.accountIds.has(card.account.id) || isTerminal(card.status))
        continue
      if (status === 'stopped')
        finishCard(card, 'stopped', run)
      else failCard(card, run, reason)
    }
  }

  async function execute(accountIds: string[], params: CompareTestParams, batch: boolean) {
    const run: CompareRun = {
      controller: new AbortController(),
      accountIds: new Set(accountIds),
      startedAtMs: performance.now(),
      batch,
      session: session.value,
    }
    resetCardsForRun(run.accountIds, params)
    runs.value = [...runs.value, run]
    try {
      await streamAccountConnectionTests(
        {
          accountIds,
          modelId: params.modelId,
          inputText: params.inputText,
          reasoningEffort: connectionTestReasoningEffort(params.reasoning),
        },
        { signal: run.controller.signal, onEvent: event => handleEvent(run, event) },
      )
      settleRun(run, 'error', '连接中断，结果未返回')
    }
    catch (error: unknown) {
      if (run.controller.signal.aborted) {
        settleRun(run, 'stopped', '')
      }
      else {
        const message = errorMessage(error, '测试请求失败')
        settleRun(run, 'error', message)
        // 401 已在接口层提示
        if (!(error instanceof ApiError && error.status === 401))
          toast.error(message)
      }
    }
    finally {
      run.controller.abort()
      runs.value = runs.value.filter(item => item !== run)
      // 测试可能改变账号状态，结束后静默刷新列表；会话失效也由这里的统一处理接管
      void options.reload()
    }
  }

  function startAll() {
    if (!canStart.value)
      return
    const params: CompareTestParams = {
      inputText: form.value.inputText.trim(),
      modelId: form.value.modelId.trim(),
      reasoning: form.value.reasoning,
    }
    void execute(cards.value.map(card => card.account.id), params, true)
  }

  function canRetry(card: CompareCard) {
    return !batchRunning.value && !cardRunning(card) && Boolean(card.params)
  }

  function retry(card: CompareCard) {
    if (!canRetry(card) || !card.params)
      return
    void execute([card.account.id], card.params, false)
  }

  function cardRunning(card: CompareCard) {
    return activeRuns.value.some(run => run.accountIds.has(card.account.id))
  }

  function stopAll() {
    for (const run of runs.value)
      run.controller.abort()
  }

  // 默认 pre 时机：父组件同一轮更新的账号列表已经到位后再重建卡片
  watch(open, (value) => {
    if (value) {
      prepare(options.accounts.value)
      return
    }
    modelsGeneration += 1
    stopAll()
  }, { immediate: true })

  onBeforeUnmount(stopAll)

  return {
    cards,
    form,
    modelHints,
    total,
    completedCount,
    batchRunning,
    anyRunning,
    tooMany,
    inputTooLong,
    canStart,
    cardElapsedMs,
    cardRunning,
    canRetry,
    startAll,
    stopAll,
    retry,
  }
}
