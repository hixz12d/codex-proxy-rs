<script setup lang="ts">
import type { EgressDirectory, EgressMutation, EgressProxy } from '@/api/modules/egress'
import { ArrowLeft, Network, Pencil, Plus, RefreshCw, Save, Trash2 } from '@lucide/vue'
import { computed, ref, watch } from 'vue'
import { getEgress, updateEgress } from '@/api/modules/egress'
import BaseButton from '@/components/base/BaseButton.vue'
import BaseFormItem from '@/components/base/BaseForm/FormItem.vue'
import BaseIconButton from '@/components/base/BaseIconButton.vue'
import BaseInput from '@/components/base/BaseInput.vue'
import BaseModal from '@/components/base/BaseModal/index.vue'
import BaseSelect from '@/components/base/BaseSelect.vue'
import BaseSwitch from '@/components/base/BaseSwitch.vue'
import { toast } from '@/components/base/BaseToast'

const props = defineProps<{ accountIds: string[] }>()
const emit = defineEmits<{ changed: [] }>()
const open = defineModel<boolean>({ required: true })
const directory = ref<EgressDirectory | null>(null)
const loading = ref(false)
const saving = ref(false)
const error = ref('')
const mode = ref<'list' | 'edit' | 'delete'>('list')
const editing = ref<EgressProxy | null>(null)
const name = ref('')
const endpoint = ref('')
const enabled = ref(true)
const proxyId = ref('__choose__')
let generation = 0
const binding = computed(() => props.accountIds.length > 0)
const title = computed(() => binding.value ? `绑定出口 · ${props.accountIds.length} 个账号` : mode.value === 'edit' ? (editing.value ? '编辑代理节点' : '新增代理节点') : mode.value === 'delete' ? '删除代理节点' : '出口代理')
const proxyOptions = computed(() => [
  { label: '选择出口', value: '__choose__', disabled: true },
  { label: '直连', value: '' },
  ...(directory.value?.proxies ?? []).map(p => ({ label: p.enabled ? p.name : `${p.name}（已停用）`, value: p.id, disabled: !p.enabled })),
])
const canBind = computed(() => proxyId.value === '' || directory.value?.proxies.some(p => p.id === proxyId.value && p.enabled))

async function load() {
  const current = ++generation
  loading.value = true
  error.value = ''
  directory.value = null
  try {
    const result = await getEgress()
    if (current !== generation || !open.value)
      return
    directory.value = result
    const selected = new Set(props.accountIds.map(id => result.bindings[id] ?? ''))
    proxyId.value = selected.size === 1 ? [...selected][0]! : '__choose__'
  }
  catch {
    if (current === generation)
      error.value = '代理配置加载失败'
  }
  finally {
    if (current === generation)
      loading.value = false
  }
}

function edit(proxy: EgressProxy | null) {
  editing.value = proxy
  name.value = proxy?.name ?? ''
  endpoint.value = ''
  enabled.value = proxy?.enabled ?? true
  error.value = ''
  mode.value = 'edit'
}

function back() {
  mode.value = 'list'
  endpoint.value = ''
  error.value = ''
}

async function mutate(command: EgressMutation) {
  if (saving.value)
    return
  saving.value = true
  error.value = ''
  try {
    await updateEgress(command)
    endpoint.value = ''
    emit('changed')
    toast.success(binding.value ? '账号出口已更新' : '代理配置已更新')
    if (binding.value) {
      open.value = false
    }
    else {
      back()
      await load()
    }
  }
  catch {
    error.value = '保存失败，请核对节点状态、绑定关系或版本后重试'
  }
  finally {
    saving.value = false
  }
}

function save() {
  if (!name.value.trim() || (!editing.value && !endpoint.value.trim()))
    return
  void mutate({
    action: 'save',
    id: editing.value?.id,
    expectedRevision: editing.value?.revision,
    name: name.value.trim(),
    endpoint: endpoint.value.trim() || undefined,
    enabled: enabled.value,
  })
}

watch(open, (value) => {
  mode.value = 'list'
  editing.value = null
  endpoint.value = ''
  proxyId.value = '__choose__'
  if (value)
    void load()
  else
    generation++
})
</script>

<template>
  <BaseModal v-model="open" :title="title" :size="binding ? 'md' : 'lg'" :dismissible="!saving">
    <div class="grid min-w-0 gap-4">
      <div v-if="error" role="alert" class="flex min-w-0 items-center justify-between gap-3 text-sm text-cp-error-text">
        <span class="min-w-0 break-words">{{ error }}</span>
        <BaseIconButton label="重新加载代理配置" :disabled="saving" @click="back(); load()">
          <RefreshCw />
        </BaseIconButton>
      </div>
      <div v-if="loading" role="status" class="py-8 text-center text-cp-text-secondary">
        加载中
      </div>
      <template v-else-if="directory">
        <BaseFormItem v-if="binding" label="账号出口">
          <BaseSelect v-model="proxyId" class="w-full" :options="proxyOptions" :disabled="saving" aria-label="账号出口" />
        </BaseFormItem>
        <form v-else-if="mode === 'edit'" id="egress-proxy-form" class="grid gap-4" @submit.prevent="save">
          <BaseFormItem label="节点名称">
            <BaseInput v-model="name" aria-label="节点名称" maxlength="100" :disabled="saving" />
          </BaseFormItem>
          <BaseFormItem :label="editing ? '替换代理地址和认证' : '代理地址和认证'">
            <BaseInput v-model="endpoint" type="password" autocomplete="new-password" aria-label="代理地址和认证" maxlength="2048" :disabled="saving" :placeholder="editing ? '留空保留现有配置' : 'socks5h://user:password@host:1080'" />
          </BaseFormItem>
          <p v-if="editing" class="m-0 break-all font-mono text-xs text-cp-text-secondary">
            {{ editing.address }}
          </p>
          <div class="flex items-center justify-between gap-4">
            <span class="text-sm text-cp-text-secondary">节点启用</span>
            <BaseSwitch v-model="enabled" label="节点启用" :disabled="saving" />
          </div>
          <p v-if="editing && !enabled && editing.accountCount" role="alert" class="m-0 text-sm text-cp-warning-text">
            停用后，已绑定的 {{ editing.accountCount }} 个账号将无法通过该节点请求。
          </p>
        </form>
        <div v-else-if="mode === 'delete'" class="grid gap-2">
          <p class="m-0 break-words">
            确认删除 {{ editing?.name }}？
          </p>
        </div>
        <template v-else>
          <div class="flex items-center justify-between gap-3">
            <span class="text-sm text-cp-text-secondary">{{ directory.proxies.length }} 个节点</span>
            <BaseButton variant="primary" @click="edit(null)">
              <Plus class="size-4" />新增代理
            </BaseButton>
          </div>
          <p v-if="!directory.proxies.length" class="m-0 py-8 text-center text-cp-text-secondary">
            暂无代理节点
          </p>
          <ul v-else class="m-0 grid list-none divide-y divide-cp-border p-0">
            <li v-for="proxy in directory.proxies" :key="proxy.id" class="flex min-w-0 items-center gap-3 py-3">
              <Network class="size-4 shrink-0" :class="proxy.enabled ? 'text-cp-success-text' : 'text-cp-text-tertiary'" />
              <div class="grid min-w-0 flex-1 gap-1">
                <span class="break-words text-sm font-medium">{{ proxy.name }}</span>
                <span class="break-all font-mono text-xs text-cp-text-secondary">{{ proxy.address }}</span>
                <span class="text-xs text-cp-text-tertiary">{{ proxy.enabled ? '已启用' : '已停用' }} · {{ proxy.accountCount }} 个账号</span>
              </div>
              <BaseIconButton :label="`编辑 ${proxy.name}`" @click="edit(proxy)">
                <Pencil />
              </BaseIconButton>
              <BaseIconButton :label="proxy.accountCount ? '节点仍有绑定账号' : `删除 ${proxy.name}`" variant="destructive" :disabled="proxy.accountCount > 0" @click="editing = proxy; mode = 'delete'">
                <Trash2 />
              </BaseIconButton>
            </li>
          </ul>
        </template>
      </template>
    </div>
    <template #footer>
      <BaseButton v-if="!binding && mode !== 'list'" variant="ghost" :disabled="saving" @click="back">
        <ArrowLeft class="size-4" />返回
      </BaseButton>
      <BaseButton v-else variant="ghost" :disabled="saving" @click="open = false">
        关闭
      </BaseButton>
      <BaseButton v-if="binding" variant="primary" :disabled="loading || !directory || !canBind" :loading="saving" @click="mutate({ action: 'bind', accountIds: [...accountIds], proxyId: proxyId || null })">
        <Save class="size-4" />保存出口
      </BaseButton>
      <BaseButton v-else-if="mode === 'edit'" type="submit" form="egress-proxy-form" variant="primary" :disabled="!name.trim() || (!editing && !endpoint.trim())" :loading="saving">
        <Save class="size-4" />保存节点
      </BaseButton>
      <BaseButton v-else-if="mode === 'delete' && editing" variant="destructive" :loading="saving" @click="mutate({ action: 'delete', id: editing.id, expectedRevision: editing.revision })">
        <Trash2 class="size-4" />确认删除
      </BaseButton>
    </template>
  </BaseModal>
</template>
