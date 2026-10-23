<!--
  Copyright 2024-2026 OpenJC (https://www.openjc.cc)
  
  Licensed under the Apache License, Version 2.0 (the "License");
  you may not use this file except in compliance with the License.
  You may obtain a copy of the License at
  
      http://www.apache.org/licenses/LICENSE-2.0
  
  Unless required by applicable law or agreed to in writing, software
  distributed under the License is distributed on an "AS IS" BASIS,
  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
  See the License for the specific language governing permissions and
  limitations under the License.
-->

<template>
  <!-- 生物识别失败 / 取消后的「换一种方式」选择器。
       自绘底部弹层而非 el-dialog：它可能与锁屏（z-index 9999）同时存在，
       这里用更高的 z-index 且不参与 Element Plus 的弹层管理，层级关系更可预期。 -->
  <Transition name="bio-sheet">
    <div v-if="biometricDlg.visible" class="bio-mask" @click.self="pick(null)">
      <div class="bio-sheet">
        <div class="bio-title">请选择验证方式</div>
        <div class="bio-sub">上一次验证未通过</div>
        <button v-if="has('fingerprint')" class="bio-btn" type="button" @click="pick('fingerprint')">
          使用指纹验证
        </button>
        <button v-if="has('face')" class="bio-btn" type="button" @click="pick('face')">
          使用人脸验证
        </button>
        <button class="bio-btn bio-btn--primary" type="button" @click="pick('pin')">
          使用安全密码
        </button>
        <button class="bio-btn bio-btn--ghost" type="button" @click="pick(null)">
          取消
        </button>
      </div>
    </div>
  </Transition>
</template>

<script setup lang="ts">
/**
 * 「换一种验证方式」选择器：由 utils/biometricDialog.ts 的单例驱动。
 * 生物识别识别不出来时，用户在这里改用指纹 / 人脸 / 安全密码。
 * 作者：JC
 */
import { biometricDlg, closeBiometricMethodDialog, type BiometricMethodPick } from '@/utils/biometricDialog'
import type { BiometricKind } from '@/utils/biometric'

/** 该模态是否可用（已按「本机具备 + 用户设置允许」过滤过，由打开方传入） */
function has(kind: BiometricKind): boolean {
  return biometricDlg.kinds.includes(kind)
}

function pick(v: BiometricMethodPick): void {
  closeBiometricMethodDialog(v)
}
</script>

<style scoped>
.bio-mask {
  position: fixed;
  inset: 0;
  z-index: 10050;
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  background: rgba(0, 0, 0, .45);
}
.bio-sheet {
  padding: 18px 16px calc(18px + env(safe-area-inset-bottom, 0px));
  background: var(--jc-bg-soft);
  border-top: 1px solid var(--jc-border);
  border-radius: 16px 16px 0 0;
  text-align: center;
}
.bio-title { font-size: 16px; font-weight: 600; color: var(--jc-text); }
.bio-sub { margin: 4px 0 14px; font-size: 12px; color: var(--jc-text-muted); }
.bio-btn {
  display: block;
  width: 100%;
  height: 46px;
  margin-bottom: 10px;
  border-radius: 12px;
  border: 1px solid var(--jc-border);
  background: var(--jc-bg);
  color: var(--jc-text);
  font-size: 15px;
  cursor: pointer;
}
.bio-btn:active { background: var(--jc-bg-hover); }
.bio-btn--primary {
  border-color: transparent;
  background: var(--jc-accent);
  color: #fff;
  font-weight: 600;
}
.bio-btn--ghost { margin-bottom: 0; border-color: transparent; background: transparent; color: var(--jc-text-muted); }
.bio-sheet-enter-active, .bio-sheet-leave-active { transition: opacity .2s ease; }
.bio-sheet-enter-from, .bio-sheet-leave-to { opacity: 0; }
</style>
