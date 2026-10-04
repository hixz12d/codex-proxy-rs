<script setup lang="ts">
import type { CompareCard, CompareCardStatus } from '../composables/useAccountCompareTest'
import type { AccountRow } from '../constants'

import { BaseButton, BaseInput, BaseModal, BaseScrollbar, BaseSelect, BaseTag, BaseTextarea } from '@codex-proxy/ui'
import { ChevronDown, Play, RotateCcw, Square } from '@lucide/vue'
import { toRef, useId } from 'vue'
import { COMPARE_TEST_MAX_ACCOUNTS, reasoningLabel, useAccountCompareTest } from '../composables/useAccountCompareTest'
import { CONNECTION_TEST_INPUT_MAX_LENGTH, CONNECTION_TEST_REASONING_OPTIONS } from '../composables/useAccountConnectionTest'
import AccountIdentityCell from './AccountIdentityCell.vue'

const props = defineProps<{
  accounts: AccountRow[]
}>()

const emit = defineEmits<{
  finished: []
}>()

const open = defineModel<boolean>({ default: false })
const modelListId = `compare-models-${useId()}`

const {
  cards,
  form,
  modelHints,
  total,
  completedCount,
  anyRunning,
  batchRunning,
  tooMany,
  inputTooLong,
  canStart,
  cardElapsedMs,
  cardRunning,
  canRetry,
  startAll,
  stopAll,
  retry,
} = useAccountCompareTest({
  open,
  accounts: toRef(props, 'accounts'),
  reload: () => emit('finished'),
})

const STATUS_VIEW: Record<CompareCardStatus, { label: string, type: 'neutral' | 'info' | 'success' | 'danger' | 'warning' }> = {
  pending: { label: '等待测试', type: 'neutral' },
  running: { label: '正在回答', type: 'info' },
  success: { label: '已完成', type: 'success' },
  error: { label: '失败', type: 'danger' },
  stopped: { label: '已停止', type: 'warning' },
}

function elapsedText(card: CompareCard) {
  const ms = cardElapsedMs(card)
  return ms === null ? '-' : `${(ms / 1000).toFixed(1)}s`
}

function cardMeta(card: CompareCard) {
  const params = card.params ?? form.value
  return `${params.modelId || '-'} · ${reasoningLabel(params.reasoning)}`
}
</script>

<template>
  <BaseModal
    v-model="open"
    title="多账号问答对比"
    description="所有账号使用相同问题、模型和思考强度，同时开始"
    tone="info"
    size="xl"
  >
    <div class="account-compare-test flex flex-col gap-4">
      <section class="rounded-cp-card bg-cp-fill-quaternary px-4 py-3">
        <div class="grid gap-3 lg:grid-cols-[minmax(0,1fr)_16rem]">
          <div class="grid min-w-0 gap-2">
            <span class="text-cp-sm font-heavy text-cp-text-quaternary">统一测试问题</span>
            <BaseTextarea
              v-model="form.inputText"
              aria-label="统一测试问题"
              :rows="4"
              :maxlength="CONNECTION_TEST_INPUT_MAX_LENGTH"
              placeholder="例如：你是什么模型"
              :disabled="anyRunning"
            />
          </div>
          <div class="grid min-w-0 content-start gap-3">
            <div class="grid min-w-0 gap-2">
              <span class="text-cp-sm font-heavy text-cp-text-quaternary">统一文本模型</span>
              <BaseInput
                v-model="form.modelId"
                aria-label="统一文本模型"
                placeholder="例如：gpt-5.4"
                :list="modelListId"
                autocomplete="off"
                spellcheck="false"
                :disabled="anyRunning"
              />
              <datalist :id="modelListId">
                <option v-for="model in modelHints" :key="model" :value="model" />
              </datalist>
            </div>
            <div class="grid min-w-0 gap-2">
              <span class="text-cp-sm font-heavy text-cp-text-quaternary">思考强度</span>
              <BaseSelect
                v-model="form.reasoning"
                aria-label="思考强度"
                :options="CONNECTION_TEST_REASONING_OPTIONS"
                :disabled="anyRunning"
              />
            </div>
          </div>
        </div>
        <p class="mt-3 mb-0 flex flex-col gap-0.5 text-cp-xs leading-normal font-emphasis text-cp-text-tertiary">
          <span>会真实消耗上游额度，思考强度越高越慢、越耗额度</span>
          <span>回答完成后一次显示</span>
        </p>
      </section>

      <section class="flex flex-wrap items-center justify-between gap-3">
        <div class="flex min-w-0 flex-wrap items-center gap-3">
          <span class="font-mono text-cp-sm font-heavy text-cp-text">
            已完成 {{ completedCount }} / {{ total }}
          </span>
          <span v-if="tooMany" class="text-cp-sm font-emphasis text-cp-error-text">
            一次最多 {{ COMPARE_TEST_MAX_ACCOUNTS }} 个账号
          </span>
          <span v-else-if="inputTooLong" class="text-cp-sm font-emphasis text-cp-error-text">
            测试问题最多 {{ CONNECTION_TEST_INPUT_MAX_LENGTH }} 个字符
          </span>
        </div>
        <BaseButton v-if="anyRunning" variant="destructive" @click="stopAll">
          <Square class="size-4" />
          停止测试
        </BaseButton>
        <BaseButton v-else variant="primary" :disabled="!canStart" @click="startAll">
          <Play class="size-4" />
          开始全部测试
        </BaseButton>
      </section>

      <section class="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
        <article
          v-for="card in cards"
          :key="card.account.id"
          class="flex min-w-0 flex-col gap-3 rounded-cp-card bg-cp-fill-quaternary p-4"
        >
          <header class="flex items-start justify-between gap-3">
            <div class="min-w-0 flex-1">
              <AccountIdentityCell :account="card.account" />
              <p class="mt-2 mb-0 truncate font-mono text-cp-xs font-emphasis text-cp-text-tertiary" :title="cardMeta(card)">
                {{ cardMeta(card) }}
              </p>
            </div>
            <div class="flex shrink-0 flex-col items-end gap-1.5">
              <BaseTag :type="STATUS_VIEW[card.status].type" size="sm">
                {{ STATUS_VIEW[card.status].label }}
              </BaseTag>
              <span class="font-mono text-cp-xs font-emphasis text-cp-text-tertiary">
                {{ elapsedText(card) }}
              </span>
            </div>
          </header>

          <div class="min-h-24 flex-1 rounded-lg bg-cp-bg-container px-3 py-2.5">
            <BaseScrollbar max-height="20rem">
              <!-- 纯文本插值，保留换行，不解析 HTML 或 Markdown -->
              <p
                v-if="card.content"
                class="m-0 whitespace-pre-wrap wrap-break-word text-cp-sm leading-[1.6] font-emphasis text-cp-text"
              >
                {{ card.content }}
              </p>
              <p v-else class="m-0 text-cp-sm font-emphasis text-cp-text-quaternary">
                {{ card.status === 'running' ? '正在等待完整回答' : card.status === 'success' ? '上游已完成，没有返回文本内容' : '-' }}
              </p>
            </BaseScrollbar>
            <p v-if="card.truncated" class="mt-2 mb-0 text-cp-xs font-emphasis text-cp-warning-text">
              回答过长已截断
            </p>
          </div>

          <div v-if="card.status === 'error'" class="rounded-lg bg-cp-error-container px-3 py-2.5">
            <p class="m-0 text-cp-sm font-heavy text-cp-error-on-container">
              {{ card.failureText }}
            </p>
            <details v-if="card.failureDetail" class="group mt-2">
              <summary
                class="flex cursor-pointer list-none items-center gap-1 rounded-cp text-cp-xs font-emphasis text-cp-error-on-container outline-none focus-visible:ring-2 focus-visible:ring-cp-control-outline [&::-webkit-details-marker]:hidden"
              >
                原始错误
                <ChevronDown class="size-3.5 transition-transform group-open:rotate-180 motion-reduce:transition-none" aria-hidden="true" />
              </summary>
              <pre
                class="mt-2 mb-0 max-h-60 overflow-auto whitespace-pre-wrap wrap-break-word rounded-lg bg-cp-bg-container px-3 py-2 font-mono text-cp-xs leading-[1.6] font-emphasis text-cp-text cp-scrollbar"
                v-text="card.failureDetail"
              />
            </details>
          </div>

          <footer class="flex justify-end">
            <BaseButton
              variant="ghost"
              size="sm"
              :loading="cardRunning(card) && !batchRunning"
              :disabled="!canRetry(card)"
              @click="retry(card)"
            >
              <RotateCcw class="size-3.5" />
              重试此账号
            </BaseButton>
          </footer>
        </article>
      </section>
    </div>

    <template #footer>
      <BaseButton variant="secondary" @click="open = false">
        关闭
      </BaseButton>
    </template>
  </BaseModal>
</template>

<style scoped>
/* 卡片多列并排，弹窗局部加宽到接近全屏宽；BaseModal 没有更大的尺寸档 */
:global(.cp-modal-panel:has(.account-compare-test)) {
  max-width: min(110rem, 100%);
}
</style>
