<script setup lang="ts">
import type { useAccountConnectionTest } from '../composables/useAccountConnectionTest'
import { CircleStop, Copy, Play, RefreshCw } from '@lucide/vue'
import BaseButton from '@/components/base/BaseButton.vue'
import BaseIconButton from '@/components/base/BaseIconButton.vue'
import BaseModal from '@/components/base/BaseModal/index.vue'
import BaseScrollbar from '@/components/base/BaseScrollbar.vue'
import BaseSelect from '@/components/base/BaseSelect.vue'
import BaseTextarea from '@/components/base/BaseTextarea.vue'
import { useCopyText } from '@/composables/useCopyText'
import AccountIdentityCell from './AccountIdentityCell.vue'

type ConnectionTest = ReturnType<typeof useAccountConnectionTest>

defineProps<{
  account: ConnectionTest['testingAccount']['value']
  status: ConnectionTest['connectionTestStatus']['value']
  model: string
  content: string
  logs: ConnectionTest['connectionTestLogs']['value']
  error: string
  startedAt: string
  finishedAt: string
  durationMs: number | null
  loadingModels: boolean
  refreshingModels: boolean
  modelOptions: ConnectionTest['connectionTestModelOptions']['value']
  statusView: ConnectionTest['connectionTestStatusView']['value']
}>()

const emit = defineEmits<{
  test: []
  stop: []
  refreshModels: []
}>()
const open = defineModel<boolean>({ default: false })
const selectedModel = defineModel<string>('selectedModel', { required: true })
const mode = defineModel<'quick' | 'manual'>('mode', { required: true })
const question = defineModel<string>('question', { required: true })
const reasoningEffort = defineModel<string>('reasoningEffort', { required: true })
const copyText = useCopyText()
const modeOptions = [
  { label: '连通测试', value: 'quick' },
  { label: '人工问答', value: 'manual' },
]
const effortOptions = [
  { label: '上游默认', value: 'default' },
  { label: '不思考 (none)', value: 'none' },
  { label: '最小 (minimal)', value: 'minimal' },
  { label: '低 (low)', value: 'low' },
  { label: '中 (medium)', value: 'medium' },
  { label: '高 (high)', value: 'high' },
  { label: '极高 (xhigh)', value: 'xhigh' },
]

function connectionLogClass(tone: string) {
  if (tone === 'success')
    return 'text-cp-success-text'
  if (tone === 'danger')
    return 'text-cp-error-text'
  if (tone === 'info')
    return 'text-cp-info-text'
  return 'text-cp-text-secondary'
}
</script>

<template>
  <BaseModal v-model="open" title="测试账号连接" tone="info" size="lg">
    <div v-if="account" class="flex min-w-0 flex-col gap-4">
      <AccountIdentityCell :account="account" size="lg" show-plan />

      <div class="grid min-w-0 gap-2">
        <div class="flex min-h-8 items-center justify-between gap-3">
          <span class="text-cp-sm font-heavy text-cp-text-secondary">测试模型</span>
          <BaseIconButton
            variant="ghost" size="sm" label="刷新上游模型"
            :loading="refreshingModels"
            :disabled="status === 'running' || loadingModels"
            @click="emit('refreshModels')"
          >
            <RefreshCw class="size-3.5" />
          </BaseIconButton>
        </div>
        <BaseSelect
          v-model="selectedModel" aria-label="测试模型"
          :options="modelOptions"
          :disabled="status === 'running' || loadingModels || refreshingModels"
          :placeholder="loadingModels ? '加载模型中...' : '选择上游模型'"
          empty-text="上游没有返回模型"
        />
      </div>

      <div class="grid min-w-0 gap-4 sm:grid-cols-2">
        <div class="grid min-w-0 gap-2">
          <span class="text-cp-sm font-heavy text-cp-text-secondary">测试模式</span>
          <BaseSelect
            v-model="mode" aria-label="测试模式" :options="modeOptions"
            :disabled="status === 'running'"
          />
        </div>
        <div v-if="account.provider === 'openai'" class="grid min-w-0 gap-2">
          <span class="text-cp-sm font-heavy text-cp-text-secondary">思考强度</span>
          <BaseSelect
            v-model="reasoningEffort" aria-label="思考强度" :options="effortOptions"
            :disabled="status === 'running' || loadingModels || refreshingModels"
          />
        </div>
      </div>

      <div v-if="mode === 'manual'" class="grid gap-2">
        <div class="flex items-center justify-between gap-2">
          <span id="connection-test-question-label" class="text-cp-sm font-heavy text-cp-text-secondary">检测问题</span>
          <span class="text-cp-xs text-cp-text-quaternary">{{ question.length }} / 8000</span>
        </div>
        <BaseTextarea
          id="connection-test-question" v-model="question" :rows="4" :maxlength="8000"
          aria-labelledby="connection-test-question-label"
          :disabled="status === 'running'" :aria-invalid="question.length > 8000"
        />
      </div>

      <p v-if="error" role="alert" class="m-0 text-cp-sm wrap-break-word text-cp-error-text">
        {{ error }}
      </p>

      <section aria-label="测试结果" class="min-w-0 border-t border-cp-border pt-4">
        <div class="flex items-center justify-between gap-3">
          <div class="flex min-w-0 items-center gap-2" role="status">
            <component :is="statusView.icon" class="size-4 shrink-0" :class="statusView.iconClass" />
            <span class="text-cp-sm font-heavy text-cp-text">{{ statusView.label }}</span>
          </div>
          <BaseIconButton
            variant="ghost" size="sm" label="复制回答" :disabled="!content"
            @click="copyText(content, { successText: '回答已复制' })"
          >
            <Copy class="size-4" />
          </BaseIconButton>
        </div>
        <p v-if="status === 'cancelled'" class="mt-1 mb-2 text-cp-xs text-cp-text-secondary">
          {{ statusView.description }}
        </p>
        <BaseScrollbar max-height="340px">
          <div class="min-h-24 py-3">
            <pre
              v-if="content"
              class="m-0 whitespace-pre-wrap [overflow-wrap:anywhere] font-mono text-cp-sm leading-relaxed text-cp-text"
              v-text="content"
            />
            <p v-else class="m-0 text-cp-sm text-cp-text-quaternary">
              {{ status === 'running' ? '等待上游回答...' : status === 'success' ? '上游已完成，没有返回文本内容' : '暂无回答' }}
            </p>
          </div>
        </BaseScrollbar>
        <dl class="m-0 grid grid-cols-2 gap-3 border-t border-cp-border py-3 text-cp-xs sm:grid-cols-3">
          <div class="min-w-0">
            <dt class="text-cp-text-quaternary">
              开始时间
            </dt>
            <dd class="mt-1 ml-0 wrap-break-word text-cp-text-secondary">
              {{ startedAt || '-' }}
            </dd>
          </div>
          <div class="min-w-0">
            <dt class="text-cp-text-quaternary">
              完成时间
            </dt>
            <dd class="mt-1 ml-0 wrap-break-word text-cp-text-secondary">
              {{ finishedAt || '-' }}
            </dd>
          </div>
          <div class="min-w-0">
            <dt class="text-cp-text-quaternary">
              响应耗时
            </dt>
            <dd class="mt-1 ml-0 text-cp-text-secondary">
              {{ durationMs !== null ? `${durationMs} ms` : '-' }}
            </dd>
          </div>
        </dl>
        <p v-if="model" class="m-0 [overflow-wrap:anywhere] font-mono text-cp-xs text-cp-text-secondary">
          {{ model }}
        </p>
      </section>

      <details v-if="logs.length" class="min-w-0 border-t border-cp-border pt-3">
        <summary class="cursor-pointer text-cp-sm font-heavy text-cp-text-secondary">
          事件轨迹
        </summary>
        <BaseScrollbar max-height="260px">
          <div class="flex flex-col gap-2 pt-3">
            <div v-for="item in logs" :key="item.key" class="grid grid-cols-[54px_minmax(0,1fr)] gap-2 text-cp-xs">
              <span class="font-mono text-cp-text-quaternary">{{ item.time }}</span>
              <div class="min-w-0">
                <p class="m-0 wrap-break-word" :class="connectionLogClass(item.tone)">
                  {{ item.text }}
                </p>
                <pre
                  v-if="item.detail"
                  class="mt-1 mb-0 whitespace-pre-wrap [overflow-wrap:anywhere] font-mono leading-relaxed text-cp-text-secondary"
                  v-text="item.detail"
                />
              </div>
            </div>
          </div>
        </BaseScrollbar>
      </details>
    </div>

    <template #footer>
      <BaseButton variant="ghost" @click="open = false">
        关闭
      </BaseButton>
      <BaseButton v-if="status === 'running'" variant="secondary" @click="emit('stop')">
        <CircleStop class="size-4" />
        停止测试
      </BaseButton>
      <BaseButton
        v-else variant="primary"
        :disabled="!account || loadingModels || refreshingModels || !selectedModel || (mode === 'manual' && (!question.trim() || question.length > 8000))"
        @click="emit('test')"
      >
        <Play class="size-4" />
        {{ logs.length > 0 ? '重新测试' : '开始测试' }}
      </BaseButton>
    </template>
  </BaseModal>
</template>
