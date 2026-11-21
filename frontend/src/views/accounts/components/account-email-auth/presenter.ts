import type { Account } from '@/api'
import type { EmailAuthBatch, EmailAuthBatchState, EmailAuthItem, EmailAuthItemState } from '@/api/modules/email-auth'
import { computed } from 'vue'
import { t } from '@/i18n'
import { numberLocale } from '@/utils/format'

export const batchStateMeta = computed<Record<EmailAuthBatchState, { label: string, badgeClass: string }>>(() => ({
  running: {
    label: t('accounts.emailAuth.batchStates.running'),
    badgeClass: 'bg-z-primary-container text-z-primary-on-container',
  },
  stopping: {
    label: t('accounts.emailAuth.batchStates.stopping'),
    badgeClass: 'bg-z-warning-container text-z-warning-on-container',
  },
  stopped: {
    label: t('accounts.emailAuth.batchStates.stopped'),
    badgeClass: 'bg-z-fill-tertiary text-z-text-secondary',
  },
  finished: {
    label: t('accounts.emailAuth.batchStates.finished'),
    badgeClass: 'bg-z-success-container text-z-success-on-container',
  },
}))

export const itemStateMeta = computed<Record<EmailAuthItemState, { label: string, badgeClass: string, fillClass: string }>>(() => ({
  succeeded: {
    label: t('accounts.emailAuth.itemStates.succeeded'),
    badgeClass: 'bg-z-success-container text-z-success-on-container',
    fillClass: 'bg-z-success',
  },
  skipped: {
    label: t('accounts.emailAuth.itemStates.skipped'),
    badgeClass: 'bg-z-info-container text-z-info-on-container',
    fillClass: 'bg-z-info',
  },
  failed: {
    label: t('accounts.emailAuth.itemStates.failed'),
    badgeClass: 'bg-z-error-container text-z-error-on-container',
    fillClass: 'bg-z-error',
  },
  stopped: {
    label: t('accounts.emailAuth.itemStates.stopped'),
    badgeClass: 'bg-z-fill-tertiary text-z-text-secondary',
    fillClass: 'bg-z-text-quaternary',
  },
  running: {
    label: t('accounts.emailAuth.itemStates.running'),
    badgeClass: 'bg-z-primary-container text-z-primary-on-container',
    fillClass: 'bg-z-primary',
  },
  pending: {
    label: t('accounts.emailAuth.itemStates.pending'),
    badgeClass: 'bg-z-fill-alter text-z-text-secondary',
    fillClass: 'bg-z-fill-secondary',
  },
}))

export const emailAuthOutcomeOrder: EmailAuthItemState[] = [
  'succeeded',
  'skipped',
  'failed',
  'stopped',
  'running',
  'pending',
]

export function emailAuthProcessedCount(batch: EmailAuthBatch) {
  return Math.max(0, batch.counts.total - batch.counts.pending - batch.counts.running)
}

export function formatEmailAuthTime(value: string) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime()))
    return value
  return new Intl.DateTimeFormat(numberLocale(), {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  }).format(date)
}

export function emailAuthItemMessage(item: EmailAuthItem) {
  // 48team 返回给人看的中文说明原样显示，不按 code 翻译
  const message = item.message?.trim()
  return message || itemStateMeta.value[item.state].label
}

export function resolveRemoteAccountDisplay(
  item: EmailAuthItem,
  accountsById: ReadonlyMap<string, Account>,
) {
  const remoteId = item.remoteAccountId?.trim()
  if (!remoteId)
    return ''
  const matched = accountsById.get(remoteId)
  const name = matched?.name.trim()
  return name && name.toLowerCase() !== item.email.toLowerCase()
    ? `${name} (${remoteId})`
    : remoteId
}
