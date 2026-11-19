import type { Ref } from 'vue'
import type { Account } from '@/api'
import type { IqProbeRound } from '@/api/modules/iq-probe'

import { onScopeDispose, shallowRef, watch } from 'vue'
import { getIqProbeRounds } from '@/api/modules/iq-probe'

/**
 * 账号列表每次加载或刷新后按当前页账号取降智检测结果
 * 结果只是附加信息，失败时静默并让格子显示占位，不影响列表本身
 */
export function useIqProbeRounds(accounts: Ref<Account[]>) {
  // 取数成功后才有值；未取到或失败时为 null，格子统一显示占位
  const roundsByAccount = shallowRef<Map<string, IqProbeRound[]> | null>(null)
  let controller: AbortController | undefined

  async function load(accountIds: string[]) {
    controller?.abort()
    if (accountIds.length === 0) {
      roundsByAccount.value = new Map()
      return
    }

    const current = new AbortController()
    controller = current
    try {
      const { items } = await getIqProbeRounds(accountIds, { silent: true, signal: current.signal })
      if (current.signal.aborted)
        return
      const grouped = new Map<string, IqProbeRound[]>()
      for (const round of items) {
        const list = grouped.get(round.accountId)
        if (list)
          list.push(round)
        else
          grouped.set(round.accountId, [round])
      }
      roundsByAccount.value = grouped
    }
    catch {
      if (!current.signal.aborted)
        roundsByAccount.value = null
    }
  }

  watch(accounts, (rows) => {
    void load([
      ...new Set(rows.map(row => row.id)),
    ])
  }, { immediate: true })

  onScopeDispose(() => controller?.abort())

  return { roundsByAccount }
}
