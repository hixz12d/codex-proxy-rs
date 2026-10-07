<script setup lang="ts">
import type { ProviderRequestProfiles } from '@/api/modules/client-profiles'
import { BaseCard, BaseSwitch } from '@codex-proxy/ui'
import ProviderRequestProfilesEditor from '@/components/client-profile/ProviderRequestProfilesEditor.vue'

withDefaults(defineProps<{ active?: boolean, disabled?: boolean }>(), {
  active: true,
  disabled: false,
})

const model = defineModel<ProviderRequestProfiles>({ required: true })
const accountFingerprintEnabled = defineModel<boolean>('accountFingerprintEnabled', { required: true })
</script>

<template>
  <BaseCard>
    <ProviderRequestProfilesEditor
      v-model="model"
      :active="active"
      :disabled="disabled"
      class="max-w-6xl"
    >
      <template #heading>
        <div class="min-w-0 pt-0.5">
          <h2 class="m-0 text-xl leading-[1.15] font-heavy text-cp-text text-balance">
            客户端身份
          </h2>
          <p class="mt-1.75 mb-0 text-cp leading-[1.3] font-emphasis text-cp-text-secondary text-pretty">
            配置网关向上游声明的客户端类型、版本与请求头
          </p>
          <!-- 开启后全局身份仍用于没有账号的请求，所以下方选择器保持可编辑，不随开关禁用。 -->
          <div class="mt-4 grid gap-1.5">
            <BaseSwitch
              v-model="accountFingerprintEnabled"
              class="justify-self-start"
              label="每个账号独立指纹"
              show-label
              :disabled="disabled"
            />
            <p class="m-0 text-cp-xs leading-[1.4] text-cp-text-secondary text-pretty">
              开启后，每个 OpenAI 账号使用由自身固定生成的 Desktop 客户端身份；全局身份和 Key 单独覆盖不再作用于账号请求。关闭恢复全局身份。修改后约 30 秒生效
            </p>
          </div>
        </div>
      </template>
    </ProviderRequestProfilesEditor>
  </BaseCard>
</template>
