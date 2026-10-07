import type { OutboundProxyRecord } from '@/api'
import { getProxies } from '@/api'

// 新账号“随机代理”只从最近一次出口测试成功且有 IPv4 出口的代理中挑选，避免新号落到 IPv6 或失效出口。
export async function loadRandomProxyPool() {
  const first = await getProxies({ page: 1, pageSize: 200 })
  const items: OutboundProxyRecord[] = [...first.items]
  for (let page = 2; page <= first.page.totalPages; page += 1)
    items.push(...(await getProxies({ page, pageSize: 200 })).items)
  const pool = items.filter(proxy => proxy.lastTest?.success && proxy.lastTest.exitIpv4)
  if (!pool.length)
    throw new Error('没有测试通过的 IPv4 代理，请先在代理页测试或改选具体代理')
  return pool
}

export function pickRandomProxy(pool: OutboundProxyRecord[]) {
  return pool[Math.floor(Math.random() * pool.length)]!.id
}
