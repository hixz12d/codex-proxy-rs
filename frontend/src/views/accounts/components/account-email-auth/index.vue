<script setup lang="ts">
import type { Account } from '@/api'
import type { EmailAuthBatch } from '@/api/modules/email-auth'

import { ZButton, ZConfirmDialog, ZDialog, ZEmpty, ZScrollbar } from '@codex-proxy/ui'
import { ArrowUpRight, Mail, RotateCcw, Square } from '@lucide/vue'
import { computed, shallowRef, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  batchStateMeta,
  emailAuthItemMessage,
  emailAuthOutcomeOrder,
  emailAuthProcessedCount,
  formatEmailAuthTime,
  itemStateMeta,
  resolveRemoteAccountDisplay,
} from './presenter'

const props = defineProps<{
  batch: EmailAuthBatch | null
  accounts: Account[]
  loading: boolean
  stopping: boolean
  retryingEmails: Set<string>
  error: string
}>()

const emit = defineEmits<{
  stop: []
  retry: [email: string]
  viewAccounts: []
  afterLeave: []
}>()

const { t } = useI18n({ useScope: 'global' })

const open = defineModel<boolean>({ required: true })
const confirmStopOpen = shallowRef(false)

const accountsById = computed(() => {
  const map = new Map<string, Account>()
  for (const account of props.accounts)
    map.set(account.id, account)
  return map
})

const countSummaryItems = computed(() => {
  const batch = props.batch
  if (!batch)
    return []
  const items = [
    {
      key: 'succeeded',
      dotClass: itemStateMeta.value.succeeded.fillClass,
      text: t('accounts.emailAuth.counts.succeeded', { count: batch.counts.succeeded }),
    },
    {
      key: 'skipped',
      dotClass: itemStateMeta.value.skipped.fillClass,
      text: t('accounts.emailAuth.counts.skipped', { count: batch.counts.skipped }),
    },
    {
      key: 'failed',
      dotClass: itemStateMeta.value.failed.fillClass,
      text: t('accounts.emailAuth.counts.failed', { count: batch.counts.failed }),
    },
  ]
  if (batch.counts.running > 0) {
    items.push({
      key: 'running',
      dotClass: itemStateMeta.value.running.fillClass,
      text: t('accounts.emailAuth.counts.running', { count: batch.counts.running }),
    })
  }
  items.push({
    key: 'pending',
    dotClass: itemStateMeta.value.pending.fillClass,
    text: t('accounts.emailAuth.counts.pending', { count: batch.counts.pending }),
  })
  if (batch.counts.stopped > 0) {
    items.push({
      key: 'stopped',
      dotClass: itemStateMeta.value.stopped.fillClass,
      text: t('accounts.emailAuth.counts.stopped', { count: batch.counts.stopped }),
    })
  }
  return items
})

watch(open, (isOpen) => {
  if (!isOpen)
    confirmStopOpen.value = false
})

watch(() => props.stopping, (busy, wasBusy) => {
  if (wasBusy && !busy)
    confirmStopOpen.value = false
})

watch(() => props.batch?.state, (state) => {
  if (state !== 'running')
    confirmStopOpen.value = false
})
</script>

<template>
  <ZDialog
    v-model="open"
    :title="t('accounts.emailAuth.progressTitle')"
    :description="t('accounts.emailAuth.progressDescription')"
    width="48rem"
    @closed="emit('afterLeave')"
  >
    <ZEmpty
      v-if="!batch && !error"
      class="min-h-[min(20rem,50dvh)] content-center"
      :icon="Mail"
      :title="loading ? t('accounts.emailAuth.loadingBatch') : t('accounts.emailAuth.emptyTitle')"
      :description="loading ? undefined : t('accounts.emailAuth.emptyDescription')"
    />

    <section v-else-if="batch" class="min-h-[min(20rem,50dvh)] min-w-0 space-y-5">
      <div class="flex flex-wrap items-start justify-between gap-3">
        <div class="min-w-0">
          <div class="flex flex-wrap items-center gap-2.5">
            <span
              class="inline-flex items-center rounded-z-sm px-2.5 py-1 text-xs font-semibold"
              :class="batchStateMeta[batch.state].badgeClass"
            >
              {{ batchStateMeta[batch.state].label }}
            </span>
            <span class="font-mono text-xs text-z-text-tertiary">
              {{ batch.id }}
            </span>
          </div>
          <p class="mt-1.5 mb-0 text-xs text-z-text-secondary">
            {{ t('accounts.emailAuth.startedAt', { time: formatEmailAuthTime(batch.createdAt) }) }}
          </p>
        </div>

        <ZButton
          v-if="batch.state === 'running'"
          size="small"
          :loading="stopping"
          @click="confirmStopOpen = true"
        >
          <template #icon>
            <Square class="size-3 text-z-error" aria-hidden="true" />
          </template>
          {{ t('accounts.emailAuth.stop') }}
        </ZButton>
        <ZButton
          v-else-if="batch.state === 'finished' || batch.state === 'stopped'"
          size="small"
          type="primary"
          variant="plain"
          @click="emit('viewAccounts')"
        >
          {{ t('accounts.emailAuth.viewAccounts') }} <ArrowUpRight class="size-3.5" aria-hidden="true" />
        </ZButton>
      </div>

      <div>
        <div class="mb-2 flex flex-wrap items-baseline justify-between gap-3 text-xs">
          <div class="flex flex-wrap gap-x-4 gap-y-1.5 text-z-text-secondary">
            <span v-for="entry in countSummaryItems" :key="entry.key" class="inline-flex items-center gap-1.5 tabular-nums">
              <span class="size-1.5 rounded-full" :class="entry.dotClass" aria-hidden="true" />
              {{ entry.text }}
            </span>
          </div>
          <span class="font-mono font-semibold tabular-nums text-z-text">
            {{ t('accounts.emailAuth.counts.total', { count: batch.counts.total }) }}
          </span>
        </div>
        <div
          v-if="batch.counts.total > 0"
          class="flex h-2 overflow-hidden rounded-full bg-z-fill-alter"
          role="progressbar"
          :aria-label="t('accounts.emailAuth.progressLabel')"
          :aria-valuenow="emailAuthProcessedCount(batch)"
          :aria-valuemax="batch.counts.total"
          :aria-valuemin="0"
        >
          <div
            v-for="status in emailAuthOutcomeOrder"
            :key="status"
            :class="itemStateMeta[status].fillClass"
            :style="{ width: `${batch.counts[status] / batch.counts.total * 100}%` }"
          />
        </div>
      </div>

      <ZScrollbar :key="batch.id" max-height="min(22rem, 42dvh)">
        <ol class="m-0 list-none space-y-1 p-0" :aria-label="t('accounts.emailAuth.itemListLabel')">
          <li
            v-for="item in batch.items"
            :key="item.email"
            class="flex flex-wrap items-center justify-between gap-x-3 gap-y-1.5 rounded-z-sm px-3 py-2.5 odd:bg-z-fill-quaternary"
          >
            <div class="min-w-0 flex-1 basis-56">
              <div class="flex flex-wrap items-center gap-2">
                <span class="font-mono text-xs font-medium text-z-text wrap-anywhere">{{ item.email }}</span>
                <span
                  v-if="item.state === 'succeeded' && resolveRemoteAccountDisplay(item, accountsById)"
                  class="rounded-z-sm bg-z-fill-tertiary px-1.5 py-0.5 font-mono text-[11px] text-z-text-secondary wrap-anywhere"
                >
                  {{ resolveRemoteAccountDisplay(item, accountsById) }}
                </span>
              </div>
              <p class="m-0 mt-0.5 text-xs leading-relaxed text-z-text-secondary wrap-anywhere">
                {{ emailAuthItemMessage(item) }}
              </p>
            </div>

            <div class="ml-auto flex shrink-0 items-center gap-2">
              <span
                class="inline-flex items-center rounded-z-sm px-2 py-0.5 text-[11px] font-semibold"
                :class="itemStateMeta[item.state].badgeClass"
              >
                {{ itemStateMeta[item.state].label }}
              </span>
              <ZButton
                v-if="item.retryable"
                size="small"
                :loading="retryingEmails.has(item.email)"
                :disabled="stopping"
                @click="emit('retry', item.email)"
              >
                <template #icon>
                  <RotateCcw class="size-3" aria-hidden="true" />
                </template>
                {{ t('accounts.emailAuth.retry') }}
              </ZButton>
            </div>
          </li>
        </ol>
      </ZScrollbar>
    </section>

    <template #footer>
      <ZButton @click="open = false">
        {{ t('common.actions.close') }}
      </ZButton>
    </template>
  </ZDialog>

  <ZConfirmDialog
    v-model="confirmStopOpen"
    :title="t('accounts.emailAuth.stopConfirmTitle')"
    destructive
    :confirm-text="t('accounts.emailAuth.confirmStop')"
    :loading="stopping"
    @confirm="emit('stop')"
  >
    <p class="m-0">
      {{ t('accounts.emailAuth.stopConfirmDescription') }}
    </p>
  </ZConfirmDialog>
</template>
