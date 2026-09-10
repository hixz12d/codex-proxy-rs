import request from '../request'

export interface EgressProxy {
  id: string
  name: string
  address: string
  enabled: boolean
  revision: number
  accountCount: number
}

export interface EgressDirectory {
  proxies: EgressProxy[]
  bindings: Record<string, string>
}

export type EgressMutation
  = | { action: 'save', id?: string, name: string, endpoint?: string, enabled: boolean, expectedRevision?: number }
    | { action: 'delete', id: string, expectedRevision: number }
    | { action: 'bind', accountIds: string[], proxyId: string | null }

export function getEgress() {
  return request<EgressDirectory>({ url: '/api/admin/egress', method: 'GET' })
}

export function updateEgress(data: EgressMutation) {
  return request<{ configRevision: number }>({ url: '/api/admin/egress/update', method: 'POST', data })
}
