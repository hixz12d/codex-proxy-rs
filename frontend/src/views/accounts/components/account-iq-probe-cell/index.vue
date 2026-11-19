<script setup lang="ts">
import type { IqProbeRound } from '@/api/modules/iq-probe'

import { ZPopover, ZScrollbar } from '@codex-proxy/ui'
import { computed, shallowRef } from 'vue'
import { useI18n } from 'vue-i18n'
import { formatDuration } from '@/utils/format'
import {
  formatIqProbeTime,
  IQ_PROBE_EMPTY_CELL_CLASS,
  iqProbeActionLabels,
  iqProbeActionMarkClass,
  iqProbeAttemptVerdictMeta,
  iqProbeCells,
  iqProbeKindLabels,
  iqProbeModelLabel,
  iqProbeOutcomeMeta,
  iqProbeRoundLabel,
} from './presenter'

const props = defineProps<{
  /** 新到旧；null 表示没取到结果 */
  rounds: IqProbeRound[] | null
}>()

const { t } = useI18n({ useScope: 'global' })

// 箭头贴着头部时取头部底色，贴着正文时取浮层底色
const popoverArrowSurfaceClasses = {
  top: 'bg-z-popover-header-bg',
  bottom: 'bg-z-bg-elevated',
}

const cells = computed(() => props.rounds?.length
  ? iqProbeCells(props.rounds)
  : [])
const activeRound = shallowRef<IqProbeRound>()
const activeElement = shallowRef<HTMLElement | null>(null)
const popoverOpen = shallowRef(false)

function activateRound(round: IqProbeRound, event: Event, open = false) {
  if (!(event.currentTarget instanceof HTMLElement))
    return
  activeRound.value = round
  activeElement.value = event.currentTarget
  // 点击和键盘聚焦直接打开，悬停交给浮层自己的延迟
  if (open)
    popoverOpen.value = true
}

const detail = computed(() => {
  const round = activeRound.value
  if (!round)
    return null
  return {
    time: formatIqProbeTime(round.startedAt),
    outcome: iqProbeOutcomeMeta.value[round.outcome],
    items: [
      {
        key: 'kind',
        label: t('accounts.iqProbe.kind'),
        value: iqProbeKindLabels.value[round.kind],
      },
      {
        key: 'action',
        label: t('accounts.iqProbe.action'),
        value: iqProbeActionLabels.value[round.action],
      },
    ],
    questions: round.detail.questions.map((question, index) => ({
      key: index,
      title: t('accounts.iqProbe.questionIndex', { index: index + 1 }),
      prompt: question.prompt,
      model: iqProbeModelLabel(question.model, question.reasoningEffort),
      verdict: iqProbeOutcomeMeta.value[question.verdict],
      attempts: question.attempts.map((attempt, attemptIndex) => ({
        key: attemptIndex,
        title: t('accounts.iqProbe.attemptIndex', { index: attemptIndex + 1 }),
        verdict: iqProbeAttemptVerdictMeta.value[attempt.verdict],
        answer: attempt.answer,
        error: attempt.error,
        elapsed: formatDuration(attempt.elapsedMs),
      })),
    })),
  }
})
</script>

<template>
  <span
    v-if="!cells.length"
    class="text-z-sm text-z-text-quaternary"
    :aria-label="t('accounts.iqProbe.empty')"
  >
    —
  </span>

  <ZPopover
    v-else
    v-model="popoverOpen"
    trigger="hover"
    placement="top"
    :offset="8"
    :disabled="!activeRound"
    :reference-element="activeElement"
    :arrow-surface-class="popoverArrowSurfaceClasses"
    class="max-w-full min-w-0"
    @closed="activeRound = undefined"
  >
    <template #reference>
      <div
        role="group"
        :aria-label="t('accounts.iqProbe.recentRounds', { count: rounds?.length ?? 0 })"
        class="flex items-center gap-0.5"
      >
        <template v-for="(round, index) in cells" :key="round?.id ?? `empty-${index}`">
          <span
            v-if="!round"
            aria-hidden="true"
            class="flex h-6 w-1.5 shrink-0 items-center"
          >
            <span class="block h-4 w-full rounded-xs" :class="IQ_PROBE_EMPTY_CELL_CLASS" />
          </span>
          <button
            v-else
            type="button"
            :aria-label="iqProbeRoundLabel(round)"
            :aria-expanded="popoverOpen && activeRound?.id === round.id"
            aria-haspopup="dialog"
            class="group relative flex h-6 w-1.5 shrink-0 cursor-pointer flex-col items-center justify-center border-0 bg-transparent p-0 outline-none focus-visible:rounded-xs focus-visible:ring-2 focus-visible:ring-z-control-outline"
            @mouseenter="activateRound(round, $event)"
            @focus="activateRound(round, $event, true)"
            @click="activateRound(round, $event, true)"
          >
            <!-- 复测轮在方块上方加小圆点 -->
            <span
              class="mb-px block size-1 rounded-full"
              :class="round.kind === 'recheck' ? 'bg-z-text-tertiary' : 'bg-transparent'"
            />
            <span
              class="block h-4 w-full rounded-xs transition-[filter] duration-150 group-hover:brightness-95 motion-reduce:transition-none"
              :class="iqProbeOutcomeMeta[round.outcome].cellClass"
            />
            <!-- 停号或熔断未停在方块下方加横条 -->
            <span
              class="mt-px block h-0.5 w-full rounded-full"
              :class="iqProbeActionMarkClass[round.action] ?? 'bg-transparent'"
            />
          </button>
        </template>
      </div>
    </template>

    <section
      v-if="detail"
      role="dialog"
      :aria-label="t('accounts.iqProbe.detailTitle')"
      class="w-96 overflow-hidden rounded-z-lg bg-z-bg-elevated"
    >
      <header class="flex items-center justify-between gap-3 bg-z-popover-header-bg px-3.5 py-3">
        <div class="flex min-w-0 items-center gap-2">
          <span class="size-2 shrink-0 rounded-full" :class="detail.outcome.cellClass" />
          <strong class="font-mono text-z leading-none font-heavy tabular-nums text-z-text">
            {{ detail.time }}
          </strong>
        </div>
        <span
          class="inline-flex h-5 shrink-0 items-center rounded-full px-2 text-[10px] leading-none font-heavy"
          :class="detail.outcome.badgeClass"
        >
          {{ detail.outcome.label }}
        </span>
      </header>

      <div class="grid gap-3 px-3.5 py-3 text-z-sm leading-5">
        <div class="grid grid-cols-2 gap-2">
          <div
            v-for="item in detail.items"
            :key="item.key"
            class="flex items-center justify-between gap-2 rounded-z bg-z-fill-quaternary px-3 py-2"
          >
            <span class="text-z-xs font-heavy text-z-text-tertiary">{{ item.label }}</span>
            <span class="font-emphasis text-z-text">{{ item.value }}</span>
          </div>
        </div>

        <p v-if="!detail.questions.length" class="m-0 text-z-text-tertiary">
          {{ t('accounts.iqProbe.noQuestions') }}
        </p>

        <ZScrollbar v-else max-height="320px">
          <div class="grid gap-2.5">
            <article
              v-for="question in detail.questions"
              :key="question.key"
              class="grid gap-2 rounded-z bg-z-fill-quaternary px-3 py-2.5"
            >
              <div class="flex items-center justify-between gap-2">
                <span class="text-z-xs font-heavy text-z-text-tertiary">{{ question.title }}</span>
                <span
                  class="inline-flex h-5 shrink-0 items-center rounded-full px-2 text-[10px] leading-none font-heavy"
                  :class="question.verdict.badgeClass"
                >
                  {{ question.verdict.label }}
                </span>
              </div>
              <p class="m-0 font-emphasis text-z-text wrap-anywhere whitespace-pre-wrap">
                {{ question.prompt }}
              </p>
              <p class="m-0 font-mono text-z-xs text-z-text-secondary wrap-anywhere">
                {{ question.model }}
              </p>

              <div
                v-for="attempt in question.attempts"
                :key="attempt.key"
                class="grid gap-1 rounded-z bg-z-fill-tertiary px-2.5 py-2"
              >
                <div class="flex items-center justify-between gap-2 text-z-xs">
                  <span class="font-heavy text-z-text-tertiary">{{ attempt.title }}</span>
                  <span class="flex items-center gap-2">
                    <span class="font-heavy" :class="attempt.verdict.textClass">{{ attempt.verdict.label }}</span>
                    <span class="font-mono tabular-nums text-z-text-quaternary">{{ attempt.elapsed }}</span>
                  </span>
                </div>
                <ZScrollbar v-if="attempt.answer" max-height="96px">
                  <p class="m-0 text-z-xs text-z-text wrap-anywhere whitespace-pre-wrap">
                    {{ attempt.answer }}
                  </p>
                </ZScrollbar>
                <p v-else-if="!attempt.error" class="m-0 text-z-xs text-z-text-quaternary">
                  {{ t('accounts.iqProbe.noAnswer') }}
                </p>
                <p v-if="attempt.error" class="m-0 text-z-xs text-z-error-text wrap-anywhere">
                  {{ attempt.error }}
                </p>
              </div>
            </article>
          </div>
        </ZScrollbar>
      </div>
    </section>
  </ZPopover>
</template>
