import type { IqProbeAction, IqProbeAttemptVerdict, IqProbeKind, IqProbeOutcome, IqProbeRound } from '@/api/modules/iq-probe'
import { computed } from 'vue'
import { t } from '@/i18n'
import { numberLocale } from '@/utils/format'

/** 与后端每号返回的最近轮数一致，固定格数让各行方块对齐 */
export const IQ_PROBE_CELL_COUNT = 24

interface IqProbeOutcomeMeta {
  label: string
  cellClass: string
  badgeClass: string
}

export const iqProbeOutcomeMeta = computed<Record<IqProbeOutcome, IqProbeOutcomeMeta>>(() => ({
  passed: {
    label: t('accounts.iqProbe.outcomes.passed'),
    cellClass: 'bg-z-success',
    badgeClass: 'bg-z-success-container text-z-success-on-container',
  },
  passed_after_retry: {
    label: t('accounts.iqProbe.outcomes.passedAfterRetry'),
    cellClass: 'bg-z-warning',
    badgeClass: 'bg-z-warning-container text-z-warning-on-container',
  },
  failed: {
    label: t('accounts.iqProbe.outcomes.failed'),
    cellClass: 'bg-z-error',
    badgeClass: 'bg-z-error-container text-z-error-on-container',
  },
  // 没测成用中性深灰，与空格子的边框色区分
  error: {
    label: t('accounts.iqProbe.outcomes.error'),
    cellClass: 'bg-z-text-quaternary',
    badgeClass: 'bg-z-fill-tertiary text-z-text-secondary',
  },
}))

export const IQ_PROBE_EMPTY_CELL_CLASS = 'bg-z-border'

/** 停号或熔断未停的格子在下方加横条，停号用正文色更醒目 */
export const iqProbeActionMarkClass: Record<IqProbeAction, string | null> = {
  none: null,
  disabled: 'bg-z-text',
  breaker_held: 'bg-z-text-tertiary',
}

export const iqProbeKindLabels = computed<Record<IqProbeKind, string>>(() => ({
  scheduled: t('accounts.iqProbe.kinds.scheduled'),
  recheck: t('accounts.iqProbe.kinds.recheck'),
}))

export const iqProbeActionLabels = computed<Record<IqProbeAction, string>>(() => ({
  none: t('accounts.iqProbe.actions.none'),
  disabled: t('accounts.iqProbe.actions.disabled'),
  breaker_held: t('accounts.iqProbe.actions.breakerHeld'),
}))

export const iqProbeAttemptVerdictMeta = computed<Record<IqProbeAttemptVerdict, { label: string, textClass: string }>>(() => ({
  pass: {
    label: t('accounts.iqProbe.attemptVerdicts.pass'),
    textClass: 'text-z-success-text',
  },
  fail: {
    label: t('accounts.iqProbe.attemptVerdicts.fail'),
    textClass: 'text-z-error-text',
  },
  unclear: {
    label: t('accounts.iqProbe.attemptVerdicts.unclear'),
    textClass: 'text-z-warning-text',
  },
  error: {
    label: t('accounts.iqProbe.attemptVerdicts.error'),
    textClass: 'text-z-text-tertiary',
  },
}))

/** 接口按新到旧返回，展示时从左到右由旧到新，不足 24 轮时左侧补空格 */
export function iqProbeCells(rounds: IqProbeRound[]) {
  const ordered = rounds.slice(0, IQ_PROBE_CELL_COUNT).reverse()
  const padding = IQ_PROBE_CELL_COUNT - ordered.length
  const cells: (IqProbeRound | null)[] = [
    ...Array.from<null>({ length: padding }).fill(null),
    ...ordered,
  ]
  return cells
}

export function formatIqProbeTime(value: string) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime()))
    return value
  return new Intl.DateTimeFormat(numberLocale(), {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  }).format(date)
}

export function iqProbeRoundLabel(round: IqProbeRound) {
  return t('accounts.iqProbe.roundLabel', {
    time: formatIqProbeTime(round.startedAt),
    kind: iqProbeKindLabels.value[round.kind],
    outcome: iqProbeOutcomeMeta.value[round.outcome].label,
    action: iqProbeActionLabels.value[round.action],
  })
}

export function iqProbeModelLabel(model: string, reasoningEffort: string | null) {
  return reasoningEffort
    ? `${model} · ${reasoningEffort}`
    : model
}
