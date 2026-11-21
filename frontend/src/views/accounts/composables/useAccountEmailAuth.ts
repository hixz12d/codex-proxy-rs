import type { EmailAuthBatch, EmailAuthState } from '@/api/modules/email-auth'
import { computed, onMounted, onScopeDispose, shallowRef, watch } from 'vue'
import { getEmailAuth, retryEmailAuth, stopEmailAuth } from '@/api/modules/email-auth'
import { useAsyncAction } from '@/composables/useAsyncAction'
import { useIdSet } from '@/composables/useIdSet'
import { t } from '@/i18n'
import { notification as ZNotification } from '@/utils/notification'
import { errorMessage } from '@/utils/operation'

export interface AccountEmailAuthStatusView {
  initialized: boolean
  configured: boolean
  isRunning: boolean
  submitError: string
}

export function useAccountEmailAuth(options: { reload: () => Promise<unknown> }) {
  const open = shallowRef(false)
  const initialized = shallowRef(false)
  const configured = shallowRef(true)
  const batch = shallowRef<EmailAuthBatch | null>(null)
  const error = shallowRef('')
  const loading = shallowRef(false)
  const stopAction = useAsyncAction()
  const retrying = useIdSet<string>()

  const isRunning = computed(() =>
    batch.value?.state === 'running' || batch.value?.state === 'stopping',
  )
  const hasBatch = computed(() => batch.value !== null)
  const activeCount = computed(() => {
    if (!batch.value || !isRunning.value)
      return 0
    return Math.max(1, batch.value.counts.pending + batch.value.counts.running)
  })

  let timer: ReturnType<typeof setTimeout> | undefined
  let controller: AbortController | undefined
  let disposed = false
  let lastBatchId = ''
  let lastSucceeded = 0

  function applyState(result: EmailAuthState) {
    const nextBatch = result.batch
    if (!initialized.value) {
      // 首次查询建立进度基线，避免历史已完成账号重复触发列表刷新
      initialized.value = true
      configured.value = result.configured
      batch.value = nextBatch
      lastBatchId = nextBatch?.id ?? ''
      lastSucceeded = nextBatch?.counts.succeeded ?? 0
      return
    }

    const baseline = nextBatch && nextBatch.id === lastBatchId
      ? lastSucceeded
      : 0
    const succeededIncreased = Boolean(nextBatch && nextBatch.counts.succeeded > baseline)
    configured.value = result.configured
    batch.value = nextBatch
    lastBatchId = nextBatch?.id ?? ''
    lastSucceeded = nextBatch?.counts.succeeded ?? 0
    if (succeededIncreased)
      void options.reload().catch(() => undefined)
  }

  function scheduleNextPoll() {
    clearTimeout(timer)
    if (disposed)
      return
    if (open.value || isRunning.value)
      timer = setTimeout(() => void refresh(true), error.value ? 10_000 : 3_000)
  }

  async function refresh(background = false) {
    clearTimeout(timer)
    controller?.abort()
    const current = new AbortController()
    controller = current
    if (!background)
      loading.value = true
    try {
      const result = await getEmailAuth({
        silent: true,
        signal: current.signal,
      })
      if (current.signal.aborted)
        return
      applyState(result)
      error.value = ''
    }
    catch (cause) {
      if (!current.signal.aborted) {
        const message = errorMessage(cause, t('accounts.emailAuth.readFailed'))
        if (open.value && (!background || error.value !== message))
          ZNotification.error({ message, deduplicate: true })
        error.value = message
      }
    }
    finally {
      if (!current.signal.aborted && !disposed) {
        loading.value = false
        scheduleNextPoll()
      }
    }
  }

  function created(result: EmailAuthState) {
    applyState(result)
    error.value = ''
    if (open.value)
      scheduleNextPoll()
    else
      open.value = true
  }

  async function stop() {
    if (!batch.value || batch.value.state !== 'running')
      return
    await stopAction.run(async () => {
      const result = await stopEmailAuth()
      applyState(result)
      error.value = ''
      scheduleNextPoll()
    })
  }

  async function retry(email: string) {
    await retrying.run(email, async () => {
      try {
        const result = await retryEmailAuth({ email })
        applyState(result)
        error.value = ''
        scheduleNextPoll()
      }
      catch {}
    })
  }

  watch(open, (isOpen) => {
    if (isOpen) {
      void refresh(Boolean(batch.value))
      return
    }
    if (!isRunning.value) {
      controller?.abort()
      clearTimeout(timer)
    }
  }, { flush: 'sync' })

  onMounted(() => void refresh())
  onScopeDispose(() => {
    disposed = true
    controller?.abort()
    clearTimeout(timer)
  })

  return {
    open,
    initialized,
    configured,
    batch,
    error,
    loading,
    isRunning,
    hasBatch,
    activeCount,
    stopping: stopAction.loading,
    retryingEmails: retrying.ids,
    created,
    refresh,
    stop,
    retry,
  }
}
