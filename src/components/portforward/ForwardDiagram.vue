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
  <div class="fwd" :class="{ 'fwd--compact': compact }">
    <div class="fwd-canvas">
      <svg
        ref="svgRef"
        class="fwd-svg"
        :class="{ 'is-dim': focusing }"
        viewBox="0 0 1400 760"
        xmlns="http://www.w3.org/2000/svg"
      >
        <defs>
          <linearGradient :id="uid + '-dev'" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stop-color="#7c8bab" />
            <stop offset="1" stop-color="#48546f" />
          </linearGradient>
          <linearGradient :id="uid + '-wall'" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stop-color="#e8584a" />
            <stop offset="1" stop-color="#b8342a" />
          </linearGradient>
          <linearGradient :id="uid + '-flame'" x1="0" y1="1" x2="0" y2="0">
            <stop offset="0" stop-color="#f97316" />
            <stop offset="0.5" stop-color="#fb9d1f" />
            <stop offset="1" stop-color="#fde047" />
          </linearGradient>
          <filter :id="uid + '-glow'" x="-90%" y="-90%" width="280%" height="280%">
            <feGaussianBlur stdDeviation="5" result="b" />
            <feMerge><feMergeNode in="b" /><feMergeNode in="SourceGraphic" /></feMerge>
          </filter>
          <!-- 节点发光：把亮色描边模糊后叠加两次，形成明显光晕（比 drop-shadow 对深色盒子有效） -->
          <filter :id="uid + '-nodeGlow'" x="-80%" y="-80%" width="260%" height="260%">
            <feGaussianBlur stdDeviation="6" result="b" />
            <feMerge>
              <feMergeNode in="b" />
              <feMergeNode in="b" />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
          <filter :id="uid + '-soft'" x="-60%" y="-60%" width="220%" height="220%">
            <feGaussianBlur stdDeviation="10" />
          </filter>
          <clipPath :id="uid + '-wallClip'"><rect x="645" :y="wallY" width="110" height="130" rx="9" /></clipPath>
          <!-- 隧道蒙版：全屏白（显示）+ 挖掉 JC Shell 节点框（黑 = 不显示）。
               管道终点刻意收在框内，LOGO 不透明时正好盖住；但编辑表单整图变暗、
               LOGO 降到半透明时，下方管道会透出来像「压住」JC Shell —— 用蒙版真正裁掉 -->
          <mask :id="uid + '-tunMask'" maskUnits="userSpaceOnUse" x="0" y="0" width="1400" height="760">
            <rect x="0" y="0" width="1400" height="760" fill="#fff" />
            <rect :x="cloudRect.x" :y="cloudRect.y" :width="cloudRect.w" :height="cloudRect.h" rx="22" fill="#000" />
          </mask>
        </defs>

        <!-- 背景 -->
        <rect x="0" y="0" width="1400" height="760" rx="16" fill="#1d2334" />

        <!-- 三个「主机父容器」：把设备、所属配置表单、服务元素都包进同一面板，
             避免子元素之间以及与其它元素互相遮挡（面板在最底层） -->
        <rect
          v-for="p in PANELS"
          :key="p.key"
          :x="p.x"
          :y="p.y"
          :width="p.w"
          :height="p.h"
          rx="26"
          class="fwd-panel fwd-el"
          :class="[`is-${nodeState(p.node)}`, { 'is-hot': highlight === p.node, 'is-on': isPanelOn(p.node), 'is-blocked': testedBlocked && p.node === 'client' && !remoteFocusNode && !extraClientEditing, 'is-editing': p.node === 'client' && (!!remoteFocusNode || extraClientEditing) }]"
          :style="isEditingNode(p.node) ? 'opacity:1' : null"
        />
        <!-- 注意：填表时只高亮对应的 SVG 图形本身；父容器只参与「是否属于当前编辑对象」的明暗判断，
             本身不点亮描边，避免和「连通性测试」的发光混淆 -->

        <!-- 老路：本地主机直连目标主机被防火墙挡住（防火墙在顶部中央，
             挡住「本地→目标」的直连路径；本地机只能经跳板机绕过去）。
             红叉标记放在「本地主机 → 防火墙」之间 -->
        <path :d="blockedLine" class="fwd-blk__line fwd-el" />
        <path :d="blockedDash" class="fwd-blk__line fwd-blk__line--dashed fwd-el" />
        <g class="fwd-blk fwd-el" :transform="props.mode === 'remote' ? 'translate(470 164)' : 'translate(470 192)'">
          <circle cx="0" cy="0" r="14" fill="#ef4444" />
          <path d="M-7 -7l14 14M7 -7l-14 14" stroke="#fff" stroke-width="3" stroke-linecap="round" />
        </g>

        <!-- 防火墙：砖墙 + 火焰（顶部中央，位于本地主机与目标主机之间，挡住直连） -->
        <g class="fwd-wall fwd-el">
          <rect x="645" :y="wallY" width="110" height="130" rx="9" :fill="'url(#' + uid + '-wall)'" />
          <g :clip-path="'url(#' + uid + '-wallClip)'">
            <rect
              v-for="b in bricks"
              :key="b.k"
              :x="b.x"
              :y="b.y"
              width="24"
              height="19"
              rx="3"
              fill="#e8534a"
            />
          </g>
          <ellipse cx="700" :cy="wallY + 122" rx="60" ry="38" fill="#f97316" opacity="0.22" :filter="'url(#' + uid + '-soft)'" />
          <g class="fwd-flame">
            <path :d="flamePath(664, wallY + 114, 30, 54)" :fill="'url(#' + uid + '-flame)'" opacity="0.9" />
            <path :d="flamePath(704, wallY + 116, 60, 108)" :fill="'url(#' + uid + '-flame)'" />
            <path :d="flamePath(698, wallY + 116, 34, 64)" fill="#ffcf3f" />
            <path :d="flamePath(694, wallY + 116, 16, 30)" fill="#fff8dc" />
          </g>
        </g>

        <!-- 绿色链路：三段路径（客户端 → JC Shell → 服务器 → 目标），虚线常驻流动。
             「SSH服务器 ↔ JC Shell」那一段（第 1 段）是真正的 SSH 隧道：画成管道（is-tunnel）并在路径下方标注「SSH 隧道」 -->
        <!-- 所有隧道连线包进同一蒙版组：JC Shell 节点框内的管道部分被裁掉，
             LOGO 变暗半透明时也不会透出下方管道 -->
        <g :mask="'url(#' + uid + '-tunMask)'">
        <g
          v-for="(d, i) in legPaths"
          :key="i"
          class="fwd-tun fwd-el"
          :class="[`is-${legState(i)}`, { 'is-tunnel': isTunnelLeg(i), 'is-blocked': isBlockedLeg(i) && i !== remoteFocusLeg, 'is-editing': i === remoteFocusLeg, 'is-sim': simLegs.has(i) }]"
          :style="i === remoteFocusLeg ? 'opacity:1' : null"
        >
          <path v-if="isTunnelLeg(i)" :d="d" class="fwd-tun__rim" />
          <path :d="d" class="fwd-tun__under" />
          <path :d="d" class="fwd-tun__over" />
          <text
            v-if="isTunnelLeg(i)"
            :x="tunnelLabel.x"
            :y="tunnelLabel.y"
            class="fwd-tun__label"
            text-anchor="middle"
          >{{ t('portForward.node.tunnel') }}</text>
        </g>

        <!-- 远程模式 + 自定义监听地址：第二台远程客户端 → SSH服务器 的连线（可达，保持绿色流动）。
             它不参与连通性测试的消息轨迹，故单独绘制、不放进 legPaths -->
        <g v-if="extraClient" class="fwd-tun fwd-el" :class="{ 'is-editing': extraClientEditing || remoteFocusNode === EXTRA_CLIENT_NODE, 'is-ok': extraState === 'ok', 'is-fail': extraState === 'fail', 'is-sim': simExtra }" :style="(extraClientEditing || remoteFocusNode === EXTRA_CLIENT_NODE) ? 'opacity:1' : null">
          <path :d="extraClient.leg" class="fwd-tun__under" />
          <path :d="extraClient.leg" class="fwd-tun__over fwd-tun__over--extra" />
        </g>
        </g>

        <!-- 双客户端：原客户端的请求被 SSH 服务器拒绝后在它那条链路上打的红叉 -->
        <g
          v-if="blockedMark.visible"
          class="fwd-blk fwd-el"
          :transform="`translate(${blockedMark.x} ${blockedMark.y})`"
        >
          <circle cx="0" cy="0" r="13" fill="#ef4444" />
          <path d="M-6.5 -6.5l13 13M6.5 -6.5l-13 13" stroke="#fff" stroke-width="3" stroke-linecap="round" />
        </g>
        <!-- 第二台远程客户端（连监听地址）不可达时，在其链路上打的红叉（与左侧各自独立，可同时显示） -->
        <g
          v-if="extraBlockedMark.visible"
          class="fwd-blk fwd-el"
          :transform="`translate(${extraBlockedMark.x} ${extraBlockedMark.y})`"
        >
          <circle cx="0" cy="0" r="13" fill="#ef4444" />
          <path d="M-6.5 -6.5l13 13M6.5 -6.5l-13 13" stroke="#fff" stroke-width="3" stroke-linecap="round" />
        </g>

        <!-- 目标主机 → 目标服务 的直连（仅本地模式有独立的目标服务元素）：
             画成实线（不流动），并作为测试轨迹的第四段，测试通过/失败时随之点亮。 -->
        <g
          v-if="mode === 'local'"
          class="fwd-tun fwd-el"
          :class="[`is-${legState(legPaths.length)}`, { 'is-on': isHot('targetService'), 'is-sim': simLegs.has(legPaths.length) }]"
        >
          <path :d="LINK_PATH" class="fwd-tun__under" />
          <path :d="LINK_PATH" class="fwd-tun__over fwd-tun__over--link" />
        </g>

        <!-- 引导线：表单区块 → 对应元素（说明这几项属于谁）。
             起点按盒子实际位置动态测量换算，保证箭头严丝合缝地连在框上 -->
        <template v-if="!compact">
          <template v-for="(ld, i) in leads" :key="i">
            <path :d="ld.d" class="fwd-lead fwd-el" :class="{ 'is-on': isLeadOn(ld.key) }" />
            <path :d="ld.head" class="fwd-lead__head fwd-el" :class="{ 'is-on': isLeadOn(ld.key) }" />
          </template>
        </template>

        <!-- 两台设备：左上=本地主机，底部中央=中间主机/跳板机 -->
        <g
          v-for="d in devices"
          :key="d.key"
          class="fwd-dev fwd-el"
          :class="[`is-${nodeState(d.node)}`, { 'is-highlight': highlight === d.node || isHot(d.node), 'is-on': isHot(d.node), 'is-blocked': !!d.blocked && d.node !== remoteFocusNode, 'is-editing': (extraClientEditing && d.node === EXTRA_CLIENT_NODE) || d.node === remoteFocusNode, 'is-sim': simNodes.has(d.node) }]"
          :style="isEditingNode(d.node) ? 'opacity:1' : null"
        >
          <rect :x="d.x + 38" :y="d.y + 136" width="44" height="8" rx="4" fill="#39415a" />
          <rect
            v-if="nodeState(d.node) !== 'idle'"
            :x="d.x" :y="d.y" width="120" height="136" rx="14"
            class="fwd-node-halo"
            :class="nodeState(d.node) === 'fail' ? 'is-fail' : 'is-ok'"
            :filter="'url(#' + uid + '-nodeGlow)'"
          />
          <rect :x="d.x" :y="d.y" width="120" height="136" rx="14" class="fwd-dev__box" :fill="'url(#' + uid + '-dev)'" />
          <rect :x="d.x + 18" :y="d.y + 24" width="84" height="12" rx="6" fill="#9fb0cd" opacity="0.9" />
          <rect :x="d.x + 18" :y="d.y + 52" width="84" height="12" rx="6" fill="#9fb0cd" opacity="0.74" />
          <rect :x="d.x + 18" :y="d.y + 80" width="84" height="12" rx="6" fill="#9fb0cd" opacity="0.58" />
          <circle :cx="d.x + 100" :cy="d.y + 120" r="6" class="fwd-dev__led" />
          <!-- 额外那台远程客户端：上方标注它的 IP（= 服务器监听地址里填的地址）。
               两台客户端都配置完成后改由 HTML 标签（IP + 编辑图标）显示，这里不再重复标注 -->
          <text v-if="d.ip && !clientsConfigured" :x="d.x + 60" :y="d.y - 12" class="fwd-dev__ip" text-anchor="middle">{{ d.ip }}</text>
          <text :x="d.x + 60" :y="d.y + 174" class="fwd-cap" text-anchor="middle">{{ d.label }}</text>
        </g>

        <!-- JC Shell 节点（原「云」）：三种模式统一用 JC Shell 的 LOGO 作图标——
             本地 / 动态模式在左下角（本地计算机侧），远程模式在 SSH 服务器右后方（本机侧）。
             节点不再画文字标题（LOGO 自带品牌文字）；「SSH 隧道」标注改放到 SSH服务器 ↔ JC Shell 的路径下方 -->
        <g
          class="fwd-cloud fwd-el"
          :class="[`is-${nodeState('internet')}`, { 'is-highlight': highlight === 'internet', 'is-sim': simNodes.has('internet') }]"
        >
          <g :transform="`translate(${cloudRect.x - 170} ${cloudRect.y - 450})`">
            <rect
              v-if="nodeState('internet') !== 'idle'"
              x="170" y="450" width="120" height="112" rx="22"
              class="fwd-node-halo"
              :class="nodeState('internet') === 'fail' ? 'is-fail' : 'is-ok'"
              :filter="'url(#' + uid + '-nodeGlow)'"
              />
              <rect x="170" y="450" width="120" height="112" rx="22" class="fwd-cloud__box" fill="#2b3450" />
              <!-- JC Shell 的 LOGO（原图原封不动、含其自带文字） -->
              <image
                :href="jcLogo"
                x="175" y="451" width="110" height="110"
                preserveAspectRatio="xMidYMid meet"
              />
          </g>
        </g>

        <!-- 目标主机（右上容器内）+ 目标服务（目标主机右侧独立元素） -->
        <g
          class="fwd-target"
          :class="[`is-${nodeState('target')}`, { 'is-highlight': highlight === 'target', 'is-sim': simNodes.has('target') }]"
        >
          <!-- 本地 / 远程模式：目标主机是一台服务器（device 造型，与本地主机一致） -->
          <template v-if="mode !== 'dynamic'">
            <g v-for="tg in targets" :key="tg.key" class="fwd-el" :class="{ 'is-on': isHot('target') }">
              <rect
                v-if="nodeState('target') !== 'idle'"
                x="1055" :y="tg.y" width="120" height="136" rx="14"
                class="fwd-node-halo"
                :class="nodeState('target') === 'fail' ? 'is-fail' : 'is-ok'"
                :filter="'url(#' + uid + '-nodeGlow)'"
              />
              <rect
                x="1055" :y="tg.y" width="120" height="136" rx="14"
                class="fwd-target__box" fill="#26314a"
                :class="{ 'is-hot': isHot('target') }"
              />
              <rect x="1073" :y="tg.y + 24" width="84" height="12" rx="6" fill="#9fb0cd" opacity="0.9" />
              <rect x="1073" :y="tg.y + 52" width="84" height="12" rx="6" fill="#9fb0cd" opacity="0.74" />
              <rect x="1073" :y="tg.y + 80" width="84" height="12" rx="6" fill="#9fb0cd" opacity="0.58" />
              <circle cx="1155" :cy="tg.y + 120" r="6" class="fwd-target__led" />
              <text x="1115" :y="tg.y + 174" class="fwd-target__text" text-anchor="middle">{{ tg.label }}</text>
            </g>
          </template>
          <!-- 动态模式：目标是「一组目标网络」，每块换成 SVG 图案（图标 + 名称）；
               整组在容器内水平、垂直居中，隧道接到容器下边缘正中而不是钻进元素里 -->
          <g v-else class="fwd-el" :class="{ 'is-on': isHot('target') }">
            <rect
              v-if="nodeState('target') !== 'idle' && dynHalo"
              :x="dynHalo.x" :y="dynHalo.y" :width="dynHalo.w" :height="dynHalo.h" rx="18"
              class="fwd-node-halo"
              :class="nodeState('target') === 'fail' ? 'is-fail' : 'is-ok'"
              :filter="'url(#' + uid + '-nodeGlow)'"
            />
            <g v-for="nt in netTiles" :key="nt.key">
              <rect class="fwd-net__chip" :x="nt.chipX" :y="nt.chipY" :width="NET_CHIP" :height="NET_CHIP" rx="14" />
              <g :transform="`translate(${nt.chipX + NET_GLYPH} ${nt.chipY + NET_GLYPH})`">
                <!-- 互联网：地球（圆 + 经线 + 两条纬线） -->
                <template v-if="nt.icon === 'globe'">
                  <circle cx="14" cy="14" r="10" class="fwd-net__stroke" />
                  <ellipse cx="14" cy="14" rx="4.4" ry="10" class="fwd-net__stroke" />
                  <path d="M4.7 10.8 H23.3 M4.7 17.2 H23.3" class="fwd-net__stroke" />
                </template>
                <!-- 内网资源：机架（两台服务器） -->
                <template v-else-if="nt.icon === 'rack'">
                  <rect x="4" y="4.5" width="20" height="8" rx="2.5" class="fwd-net__stroke" />
                  <rect x="4" y="15.5" width="20" height="8" rx="2.5" class="fwd-net__stroke" />
                  <circle cx="8.6" cy="8.5" r="1.3" class="fwd-net__dot" />
                  <circle cx="8.6" cy="19.5" r="1.3" class="fwd-net__dot" />
                </template>
                <!-- 其它网络：三节点互连 -->
                <template v-else>
                  <circle cx="14" cy="5" r="3.2" class="fwd-net__stroke" />
                  <circle cx="5.5" cy="21" r="3.2" class="fwd-net__stroke" />
                  <circle cx="22.5" cy="21" r="3.2" class="fwd-net__stroke" />
                  <path d="M11.7 7.5 L7.8 18.5 M16.3 7.5 L20.2 18.5 M8.7 21 H19.3" class="fwd-net__stroke" />
                </template>
              </g>
              <text class="fwd-net__text" :x="nt.cx" :y="nt.labelY" text-anchor="middle">{{ nt.label }}</text>
            </g>
          </g>
          <!-- 目标服务（目标主机右侧的小框，表示最终要访问的服务；仅本地模式） -->
          <g v-if="mode === 'local'" class="fwd-tgtsvc fwd-el" :class="{ 'is-on': isHot('targetService') }">
            <!-- 目标服务的发光跟随最后一段（目标主机 → 目标服务）的结果 -->
            <rect
              v-if="legState(legPaths.length) !== 'idle'"
              x="1245" y="172" width="110" height="40" rx="10"
              class="fwd-node-halo"
              :class="legState(legPaths.length) === 'fail' ? 'is-fail' : 'is-ok'"
              :filter="'url(#' + uid + '-nodeGlow)'"
            />
            <rect
              x="1245" y="172" width="110" height="40" rx="10"
              class="fwd-tgtsvc__box" fill="#1a3a32" stroke="#35d07f" stroke-width="1.5"
              :class="{ 'is-hot': isHot('targetService') }"
            />
            <text x="1300" y="197" class="fwd-tgtsvc__text" text-anchor="middle">{{ t('portForward.node.targetService') }}</text>
          </g>
        </g>

        <!-- 测试时发送的消息光球（常态不显示，仅在点击测试时出现）。
             双客户端时还有第二颗红色光球：它沿不可达的那条链路走，中途会被拦下 -->
        <template v-if="packet.visible">
          <circle class="fwd-packet-halo" :class="{ 'is-sim': packet.sim }" :cx="packet.x" :cy="packet.y" r="16" />
          <circle class="fwd-packet" :class="{ 'is-sim': packet.sim }" :cx="packet.x" :cy="packet.y" r="8" :filter="'url(#' + uid + '-glow)'" />
        </template>
        <template v-if="packet2.visible">
          <circle class="fwd-packet-halo" :class="{ 'is-blocked': packet2.rejected, 'is-sim': packet2.sim }" :cx="packet2.x" :cy="packet2.y" r="16" />
          <circle class="fwd-packet" :class="{ 'is-blocked': packet2.rejected, 'is-sim': packet2.sim }" :cx="packet2.x" :cy="packet2.y" r="8" :filter="'url(#' + uid + '-glow)'" />
        </template>
      </svg>

      <!-- 表单：每个配置组贴在对应 SVG 元素旁边 -->
      <div
        v-for="z in zones"
        :key="z.key"
        :ref="(el) => setZoneEl(el as Element | null, z.key)"
        class="fwd-zone"
        :class="{ 'is-dim': editingZone !== null && editingZone !== z.key, 'fwd-zone--col': props.mode === 'remote' && z.key === 'target', 'fwd-zone--dyn-close': props.mode === 'dynamic' && z.key === 'target' }"
        :style="z.style"
      >
        <!-- 动态模式测试目标表单：非必填，顶部右侧提供关闭按钮，关闭后隐藏；再次点击测试连通性会重新询问 -->
        <button
          v-if="props.mode === 'dynamic' && z.key === 'target'"
          type="button"
          class="fwd-zone__close"
          :title="t('common.close')"
          @click="emit('dyn-close')"
        >
          <el-icon><Close /></el-icon>
        </button>
        <!-- SSH 服务器分组：标题 + 「选择会话」按钮（点开右侧会话列表，单击回填） -->
        <div
          v-if="z.node === 'ssh'"
          class="fwd-zone__head"
          @focusin="focusEl('ssh')"
          @focusout="blurEl()"
        >
          <span class="fwd-zone__title">{{ t('portForward.node.sshServer') }}</span>
          <el-button size="small" class="fwd-pick" @click="emit('pick-session')">
            <el-icon><Connection /></el-icon>{{ t('portForward.btn.pickSession') }}
          </el-button>
        </div>

        <template v-for="f in fieldsOf(z.node)" :key="f.key">
          <!-- 认证方式：非输入框，说明文案独占一行，下面放单选按钮 -->
          <div
            v-if="f.type === 'auth'"
            class="fwd-field fwd-field--auth"
            @focusin="focusField(f)"
            @focusout="blurEl()"
          >
            <label class="fwd-field__caption">{{ t(f.labelKey) }}</label>
            <el-radio-group
              :model-value="String(form?.[f.key] ?? 'password')"
              @update:model-value="(v: string | number | boolean) => apply?.({ [f.key]: String(v) })"
            >
              <el-radio-button value="password">{{ t('portForward.auth.password') }}</el-radio-button>
              <el-radio-button value="key">{{ t('portForward.auth.key') }}</el-radio-button>
            </el-radio-group>
          </div>
          <!-- 密码：仅「密码」认证方式下出现 -->
          <div
            v-else-if="f.type === 'password' && form?.sshAuthType !== 'key'"
            class="fwd-field fwd-field--float"
            :class="{ 'is-float': isFilled(f) }"
            @focusin="focusField(f)"
            @focusout="blurEl()"
          >
            <label class="fwd-field__label">{{ t(f.labelKey) }}</label>
            <el-input
              :model-value="String(form?.[f.key] ?? '')"
              :type="passwordVisible ? 'text' : 'password'"
              @update:model-value="(v: string) => apply?.({ [f.key]: v })"
            >
              <template #suffix>
                <el-icon class="fwd-eye" @click="togglePasswordVisible">
                  <View v-if="!passwordVisible" />
                  <Hide v-else />
                </el-icon>
              </template>
            </el-input>
          </div>
          <!-- 私钥：仅「密钥」认证方式下出现（与密码互斥） -->
          <div
            v-else-if="f.type === 'key' && form?.sshAuthType === 'key'"
            class="fwd-field fwd-field--float fwd-field--key"
            :class="{ 'is-float': isFilled(f) }"
            @focusin="focusField(f)"
            @focusout="blurEl()"
          >
            <label class="fwd-field__label">{{ t(f.labelKey) }}</label>
            <div class="fwd-key-row">
              <el-input
                :model-value="form?.sshKeyName || form?.sshKeyPath || ''"
                readonly
              />
              <el-button @click="pickKeyFile">{{ t('portForward.btn.browse') }}</el-button>
            </div>
          </div>
          <!-- 端口 -->
          <div
            v-else-if="f.type === 'port'"
            class="fwd-field fwd-field--float"
            :class="{ 'is-float': isFilled(f) }"
            @focusin="focusField(f)"
            @focusout="blurEl()"
          >
            <label class="fwd-field__label">{{ t(f.labelKey) }}</label>
            <!-- 服务器监听端口：填写后在其正下方显示内联 Tip（约 10 秒后自动消失），
                 提示需在服务器放行该端口并开启 GatewayPorts；内联占位不浮层，与其它提示互不重叠。
                 端口输入采用「输入即阻断」：逐键过滤非数字、限长 5 位、超界立即截断为 65535，
                 一旦达到上限后续字符根本无法输入（t 透出当前 mode 供远程模式 Tip 复用） -->
            <el-input
              :model-value="portText(f)"
              inputmode="numeric"
              :maxlength="5"
              @update:model-value="(v: string) => onPortInput(f, v)"
            />
          </div>
          <!-- 文本 -->
          <div
            v-else-if="f.type === 'text'"
            class="fwd-field fwd-field--float"
            :class="{ 'is-float': isFilled(f) }"
            @focusin="focusField(f)"
            @focusout="blurEl()"
          >
            <label class="fwd-field__label">{{ t(f.labelKey) }}</label>
            <el-input
              :model-value="String(form?.[f.key] ?? '')"
              @update:model-value="(v: string) => apply?.({ [f.key]: v })"
            />
          </div>
        </template>

        <!-- 服务器监听端口 Tip：横跨表单整行（与表单同宽），文档流占位不盖其它提示 -->
        <template v-if="z.node === 'ssh'">
          <Transition name="fwd-tip-fade">
            <div v-if="portTipVisible" class="fwd-port-tip">
              <span class="fwd-port-tip__icon">⚠</span>
              <span>{{
                t('portForward.remoteClient.gatewayTip', {
                  host: (form?.sshHost || '').trim() || '…',
                  port: Number(portTipValue) || 0
                })
              }}</span>
            </div>
          </Transition>
        </template>

        <!-- 服务器监听地址提示（仅远程模式）：以「弹窗」形式浮在表单下方，
             只在页面加载、以及该输入框内容变化时出现，10 秒后自动消失，不常驻页面 -->
        <Transition name="fwd-tip-fade">
          <div
            v-if="z.node === 'ssh' && tipVisible && remoteBindTip"
            class="fwd-tip"
            :class="`is-${remoteBindTip.level}`"
          >
            <span class="fwd-tip__icon">{{ remoteBindTip.level === 'warn' ? '⚠' : '✓' }}</span>
            <span>{{ t(remoteBindTip.key, remoteBindTip.params) }}</span>
          </div>
        </Transition>
      </div>

      <!-- 远程模式：两台远程客户端都配置好后，在各自节点上方显示「编辑图标 + IP」标签；
           点击编辑图标重新打开该客户端的配置面板 -->
      <div
        v-for="tag in remoteClientTags"
        :key="tag.side"
        class="fwd-ctag"
        :style="tag.style"
      >
        <el-icon class="fwd-ctag__edit" @click="emit('edit-remote-client', tag.side)"><EditPen /></el-icon>
        <span class="fwd-ctag__ip">{{ tag.ip }}</span>
      </div>

      <!-- 测试结果徽标：叠加在画面上，测试结束显示，明显反馈结果 -->
      <Transition name="fwd-badge-pop">
        <div v-if="outcome" class="fwd-badge" :class="outcome.ok ? 'is-ok' : 'is-fail'">
          <span class="fwd-badge__icon">{{ outcome.ok ? '✓' : '✕' }}</span>
          <span>{{ outcome.text }}</span>
        </div>
      </Transition>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 端口转发架构示意图（彩色 SVG 场景 + 元素旁内嵌表单）
 * 作者：JC
 *
 * 画面布局（参照用户示意图，画布 1400×760，宽比例便于铺满弹窗宽度）：
 *   左上容器：本地主机(客户端)   ·   顶部中央：防火墙（砖墙+火焰）   ·   右上容器：目标主机 + 目标服务
 *   左下：云(Internet/SSH 隧道)   ·   底部中央容器：中间主机/跳板机(SSH 服务器)
 *
 * - 三个「主机父容器」把设备、所属配置表单、服务元素圈在一起，彼此不遮挡；
 * - 红色直线：本地主机 → ✕ → 防火墙 → --- → 目标主机（直连被挡住）；
 * - 绿色链路：本地计算机 → JC Shell → 跳板机 → 目标（绕过防火墙的转发路径）；
 *   三种模式的 JC Shell 节点都用 JC Shell 的 LOGO 表示（不再画文字标题），
 *   「SSH服务器 ↔ JC Shell」那一段画成管道状 SSH 隧道并在路径下方标注「SSH 隧道」，其余段都是普通连接；
 * - 动态模式：目标是「一组目标网络」，容器内放三块 SVG 图案（地球 / 机架 / 节点互连），
 *   三列等宽等距、整组在容器内水平与垂直居中，容器高度相应收小，隧道接到容器下边缘；
 * - 表单字段不再单独占一行标签：说明文案放进 placeholder（认证方式除外，它独占一行）；
 *   密码 / 私钥互斥，按认证方式只出现一个；跳板机分组含「选择会话」按钮，可从会话列表回填；
 * - 编辑聚焦：焦点落在某个输入框 / 表单元素时，只有它映射到的 SVG 元素（连同同容器的引导线）保持全亮，
 *   其余元素与其它分组一并变暗；失焦后整图恢复常态（既不高亮也不变暗）；
 * - 常驻循环动画：消息沿绿色隧道不间断跑完全程；
 * - play()：连通性测试，通过的段/节点变绿、失败变红并停在故障点。
 */
import { computed, getCurrentInstance, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { Connection, EditPen, View, Hide, Close } from '@element-plus/icons-vue'
import jcLogo from '@/assets/jc-shell-logo.png'
import { ElMessage } from 'element-plus'
import { useI18nStore } from '@/stores/i18n'
import { ensureUnlocked } from '@/utils/securityPassword'
import { invoke, isTauriEnv } from '@/api'
import type { ForwardMode, ForwardNodeId, ForwardRule, ForwardTestStep } from '@/stores/portForward'
import { buildTestSteps, remoteClientConfigured } from '@/stores/portForward'
import { FIELDS, NODES, type FieldDef, type ForwardHotKey } from './forwardMeta'

const props = withDefaults(defineProps<{
  mode: ForwardMode
  compact?: boolean
  form?: ForwardRule | null
  apply?: (patch: Partial<ForwardRule>) => void
  highlight?: ForwardNodeId | null
  /** 远程客户端配置面板联动：光标停在 left/right 表单时高亮图上对应的那台远程客户端 */
  remoteFocus?: 'left' | 'right' | null
  /** 动态模式「测试连通性」：是否已选择配置测试目标（未配置则目标区隐藏，仅走模拟） */
  dynTargetOpen?: boolean
}>(), { compact: false, form: null, apply: undefined, highlight: null, remoteFocus: null, dynTargetOpen: false })

const emit = defineEmits<{
  (e: 'pick-session'): void
  /** 点击远程客户端节点上方的编辑图标：重新打开该客户端的 SSH 配置面板 */
  (e: 'edit-remote-client', side: 'left' | 'right'): void
  /** 动态模式：点击测试目标表单的关闭按钮，隐藏该表单（非必填，可关闭） */
  (e: 'dyn-close'): void
}>()

const t = useI18nStore().t

/**
 * 每个实例独立的 SVG defs id 前缀。
 * 同一页面可能同时存在多张架构图（例如「创建」会先经「选择模式」窗口，里面 3 张图关闭后
 * DOM 仍保留），若 defs 的 id 相同，`url(#id)` 会解析到文档中先出现的那一份（可能是隐藏实例的），
 * 导致发光滤镜/渐变失效。用实例 uid 前缀即可保证各图引用各自的 defs。
 */
const uid = `fwd${getCurrentInstance()?.uid ?? Math.random().toString(36).slice(2, 9)}`

/** 动态模式的目标容器基准：里面只放三块「目标网络」图案，故比其它模式收小一圈（高度减半） */
const DYN_PANEL_BASE = { x: 935, y: 124, w: 430, h: 136 }
/** 测试目标输入框与下方图案之间的纵向间距：出现时 DYN_PANEL 向下多撑开这段，
 *  既放大容器的纵向高度，也让表单元素与目标网络图案拉开距离 */
const DYN_INPUT_H = 130
/** 动态模式的目标容器：选择「配置测试目标」后放大（输入框占顶部、图案紧贴其下） */
const DYN_PANEL = computed(() =>
  props.dynTargetOpen
    ? { ...DYN_PANEL_BASE, h: DYN_INPUT_H + NET_BLOCK_H + 16 }
    : DYN_PANEL_BASE,
)
/** 网络图案：图标方块边长、图标在方块内的留白（28×28 的图案居中放在方块里） */
const NET_CHIP = 52
const NET_GLYPH = 12
/** 三块图案各占一列：列宽与列间距。三列等宽 → 整组天然水平居中，与文字长短无关 */
const NET_COL_W = 100
const NET_COL_GAP = 30
/** 「图标 + 名称」的总高：图标方块 + 名称占位，用于纵向居中 */
const NET_BLOCK_H = NET_CHIP + 24

/**
 * 三个主机父容器面板（最底层绘制）：
 * 每个面板把「设备 + 该设备的配置表单 + 所属服务/标签」圈在一起，
 * 子元素之间以及与其它元素不会互相遮挡。
 * 动态模式的目标容器只装三块网络图案，尺寸相应收小。
 */
const PANELS = computed<Array<{ key: string; node: ForwardNodeId; x: number; y: number; w: number; h: number }>>(() => [
  // 远程模式：左侧远程客户端面板收窄并左移（该模式下面板内只有设备、没有表单）；
  // 顶部带（远程客户端 / 本机服务）整体上移压缩，为中部 SSH 表单腾出更大区域
  props.mode === 'remote'
    ? { key: 'client', node: 'client', x: 20, y: 16, w: 300, h: 272 }
    : { key: 'client', node: 'client', x: 40, y: 20, w: 380, h: 292 },
  props.mode === 'remote'
    ? { key: 'target', node: 'target', x: 935, y: 16, w: 430, h: 272 }
    : props.mode === 'dynamic'
      ? { key: 'target', node: 'target', x: DYN_PANEL.value.x, y: DYN_PANEL.value.y, w: DYN_PANEL.value.w, h: DYN_PANEL.value.h }
      : { key: 'target', node: 'target', x: 935, y: 20, w: 430, h: 292 },
  props.mode === 'remote'
    ? { key: 'ssh', node: 'ssh', x: 392, y: 232, w: 520, h: 512 }
    : { key: 'ssh', node: 'ssh', x: 410, y: 250, w: 460, h: 485 },
])

/**
 * JC Shell 节点的位置（三种模式都用 JC Shell 的 LOGO 作图标，不再画文字标题）：
 * 本地 / 动态模式在左下角（本地计算机侧），远程模式放在 SSH 服务器右后方（本机侧）。
 * 三种模式里，SSH服务器 ↔ JC Shell 那一段才是 SSH 隧道（legPaths[1]，画成管道并标注「SSH 隧道」），
 * 客户端那一段与目标那一段都是普通连接。
 * 图形内部坐标按原左下角 (170,450) 设计，靠外层 group 的 translate 整体平移。
 */
const CLOUD_LEFT = { x: 170, y: 450, w: 120, h: 112 }
const CLOUD_RIGHT = { x: 1055, y: 430, w: 120, h: 112 }
const cloudRect = computed(() => (props.mode === 'remote' ? CLOUD_RIGHT : CLOUD_LEFT))

/* ---- 远程模式：服务器监听地址（bindAddr）取值带来的差异 ---- */

/** 远程模式下的服务器监听地址（去掉首尾空格）；其它模式为空 */
const bindAddr = computed(() => (props.mode === 'remote' ? String(props.form?.bindAddr ?? '').trim() : ''))
/** 只对本机（回环）开放：127.0.0.1 / localhost */
const bindIsLoopback = computed(
  () => bindAddr.value === '127.0.0.1' || bindAddr.value.toLowerCase() === 'localhost'
)
/** 对所有来源开放 */
const bindIsAll = computed(() => bindAddr.value === '0.0.0.0')

/** 监听地址就是 SSH 服务器自身的地址（IP / 主机名）：这种取值下远程客户端同样连得上 */
const bindIsSshHost = computed(() => {
  const host = String(props.form?.sshHost ?? '').trim().toLowerCase()
  return !!host && bindAddr.value.toLowerCase() === host
})

/**
 * 第二台远程客户端的出现判定（用户每输入 / 删除一个字符都实时判断）：
 * 当前输入为空，或仍是 localhost / 127.0.0.1 / 0.0.0.0 / SSH 服务器自身地址之一的
 * 「前缀」（含完全相等）时，还无法断定用户最终会输入一个独立的自定义地址 → 不出现；
 * 一旦敲下的字符使其不再匹配任何候选地址（或其前缀）→ 立即出现。
 */
const extraClientVisible = computed(() => {
  const a = bindAddr.value.toLowerCase()
  if (!a) return false
  const candidates = ['localhost', '127.0.0.1', '0.0.0.0']
  const host = String(props.form?.sshHost ?? '').trim().toLowerCase()
  if (host) candidates.push(host)
  return !candidates.some(c => c.startsWith(a))
})

/**
 * 自定义监听地址：输入已「注定不是」回环 / 0.0.0.0 / SSH 服务器自身地址（或其前缀）时的实时取值。
 * 只有这种取值才会多出一台「同地址」的远程客户端。
 */
const customBindAddr = computed(() => (extraClientVisible.value ? bindAddr.value : ''))

/**
 * 远程客户端的请求能否抵达 SSH 服务器：
 * 只有监听地址对所有来源开放（0.0.0.0）时，原来的那台远程客户端才连得上；
 * 绑回环（127.0.0.1 / localhost）、绑服务器自身内网 IP、或绑其它自定义地址时，
 * 原来的那台远程客户端都连不进来（在服务器处被拒收）。
 * 该状态只用于测试动画的判定；界面上持久的红色阻断态由 testedBlocked（测试结果）驱动。
 */
const remoteClientBlocked = computed(
  () => props.mode === 'remote' && !!bindAddr.value && !bindIsAll.value
)

/**
 * 连通性测试已证实「原远程客户端 → SSH服务器」不通：
 * 红色阻断态（设备变暗红框 + 链路转红停止流动）由测试结果触发并持久保持，
 * 直到页面关闭或用户修改服务器监听地址，不随发光效果结束而复原。
 * 平时（未测试或测试通过）两台客户端与两条链路都是统一的绿色常态。
 */
const testedBlocked = ref(false)

/**
 * 自定义监听地址时多出的那台远程客户端：代表该地址所在网络里的客户端，
 * 只有它的请求能抵达 SSH 服务器（连线保持绿色）；原来的远程客户端则不可达（连线转红）。
 */
const extraClient = computed<{ ip: string; x: number; y: number; leg: string } | null>(() =>
  customBindAddr.value
    ? { ip: customBindAddr.value, x: 182, y: 96, leg: 'M242 232 C 242 540, 302 618, 580 618' }
    : null
)

/**
 * 第二台客户端正处于「编辑中」：它已经出现（extraClientVisible），且光标还停在监听地址输入框里。
 * 此时把它和专属路径线高亮，提示「当前正在填的 IP 就是这台客户端」。
 */
const extraClientEditing = computed(() => extraClientVisible.value && bindAddrFocused.value)

/**
 * 远程客户端配置面板联动：光标停在 left/right 哪台客户端的 SSH 表单里，
 * 就把图上对应的那台客户端点亮（琥珀色，与「正在填监听地址」的高亮同款）。
 * left = 原来那台（'client' 节点），right = 新增那台（EXTRA_CLIENT_NODE）。
 */
const remoteFocusNode = computed<ForwardNodeId | null>(() => {
  if (props.mode !== 'remote' || !props.remoteFocus) return null
  return props.remoteFocus === 'left' ? NODES.remote[0].id : EXTRA_CLIENT_NODE
})

/** 原有远程客户端设备的 x：多出一台时左移，两台左右并列 */
const clientX = computed(() => (props.mode === 'remote' ? (extraClient.value ? 38 : 110) : 170))

/**
 * 光标停在哪台远程客户端表单里，就把该客户端「到服务器」的那条链路一起点亮（黄色）：
 * left（原客户端）对应 legPaths 的第 0 段；right（新增客户端）的连线由模板单独处理。
 */
const remoteFocusLeg = computed(() => (remoteFocusNode.value === NODES.remote[0].id ? 0 : -1))

/** 某节点是否正处于「正在配置」高亮态（用于模板内联强制 opacity:1，压过任何变暗规则） */
function isEditingNode(node: ForwardNodeId): boolean {
  return (extraClientEditing.value && node === EXTRA_CLIENT_NODE) || node === remoteFocusNode.value
}

/** 远程模式：两台远程客户端均已配置完成（可发真实请求） */
const clientsConfigured = computed(
  () => props.mode === 'remote' && !props.compact && !!props.form && remoteClientConfigured(props.form),
)

/**
 * 两台远程客户端都配置好之后，在各自节点上方显示的「编辑图标 + IP」标签。
 * IP 取该客户端配置里填的「SSH 服务器」地址（即它要连过去的那台机器）。
 * left = 原客户端节点；right = 监听地址为自定义地址时多出的那台客户端节点。
 */
const remoteClientTags = computed(() => {
  const f = props.form
  if (!clientsConfigured.value || !f) return []
  const tags: Array<{ side: 'left' | 'right'; ip: string; style: Record<string, string> }> = []
  const leftIp = (f.remoteClientLeft?.sshHost || '').trim()
  if (leftIp) {
    const cx = clientX.value + 60
    tags.push({ side: 'left', ip: leftIp, style: { left: `${(cx / 1400) * 100}%`, top: `${((96 - 6) / 760) * 100}%` } })
  }
  const rightIp = (f.remoteClientRight?.cfg?.sshHost || '').trim()
  if (rightIp && extraClient.value) {
    const cx = extraClient.value.x + 60
    tags.push({ side: 'right', ip: rightIp, style: { left: `${(cx / 1400) * 100}%`, top: `${((96 - 6) / 760) * 100}%` } })
  }
  return tags
})

/**
 * 三段绿色路径。三种模式的第 1 段都是「SSH服务器 ↔ JC Shell」的 SSH 隧道（isTunnelLeg → 管道 + 文字标注），
 * 其余两段是普通连接。
 * 本地 / 动态模式：JC Shell 在左下角，第 0 段由本地计算机进 JC Shell，第 1 段贴着 SSH 表单下方绕行接跳板机，
 *   第 2 段由跳板机到目标（终点按模式区分，见下）。
 * 远程模式：JC Shell 在 SSH 服务器右后方，第 0 段为「远程客户端 → 服务器监听端口」，
 *   第 1 段「SSH服务器 → JC Shell」，第 2 段「JC Shell → 本机服务」。
 */
const legPaths = computed(() => {
  if (props.mode === 'remote') {
    return [
      // 远程客户端 → SSH 服务器：肘形连线 —— 竖直直落 → 中间一个明显的圆角拐角（R 角，半径约 110）
      //   → 水平进入设备左侧；两端切线竖直/水平，弧线平顺过渡。
      //   多出一台远程客户端时，原来这台左移，两台分别落到服务器左侧不同高度（避免连线叠在一起）
      extraClient.value
        ? 'M98 232 C 98 580, 158 660, 580 660'
        : 'M170 232 C 170 585, 244 650, 580 650',
      // SSH 服务器 → JC Shell：同一高度 y=650 出发，带明显上扬弧线；
      //   终点收进 JC Shell 节点框内（避免落在圆角上导致管道口露在外面、看着没封死）
      //   —— 这一段就是 SSH 隧道（入口=服务器监听端口），由 isTunnelLeg(1) 画成管道效果
      'M700 650 C 860 650, 972 578, 1082 522',
      // JC Shell → 本机服务：竖直一线，JC Shell「正上方」出、本机服务「正下方」进（普通本地连接）
      'M1115 430 L 1115 232',
    ]
  }
  return [
    'M230 260 L 230 448',
    'M230 562 C 230 612, 300 640, 576 620',
    props.mode === 'dynamic'
      ? `M700 630 C 900 640, ${DYN_PANEL.value.x + DYN_PANEL.value.w / 2} 480, ${DYN_PANEL.value.x + DYN_PANEL.value.w / 2} ${DYN_PANEL.value.y + DYN_PANEL.value.h}`
      : 'M700 630 C 850 630, 1030 500, 1065 262',
  ]
})

/**
 * 该段是否要画成「隧道」管道效果。
 * 三种模式的第 1 段都是「SSH服务器 ↔ JC Shell」那段 SSH 隧道，其余段（客户端侧、目标侧）都是普通连接。
 */
function isTunnelLeg(i: number): boolean {
  return i === 1
}

/** 「SSH 隧道」文字标注的位置：放在隧道路径下方、紧贴管道（各模式该段路径不同，逐模式给定） */
const tunnelLabel = computed(() =>
  props.mode === 'remote' ? { x: 906, y: 636 } : { x: 300, y: 646 }
)

/** 第 0 段（远程客户端 → SSH服务器）要呈现「阻断」态：变暗 + 转红 + 停止流动（测试证实不通后持久保持） */
function isBlockedLeg(i: number): boolean {
  return testedBlocked.value && i === 0
}

/**
 * 监听地址对应的 Tip 弹窗内容；null = 不提示。
 * 0.0.0.0 → 对所有来源开放；回环（127.0.0.1 / localhost）→ 仅服务器本机；
 * 其它地址 → 仅来自该地址的请求。
 */
const remoteBindTip = computed<{ level: 'warn' | 'ok'; key: string; params?: Record<string, string> } | null>(() => {
  if (!bindAddr.value) return null
  if (bindIsAll.value) return { level: 'ok', key: 'portForward.tip.bindAll' }
  if (bindIsLoopback.value) return { level: 'warn', key: 'portForward.tip.bindLoopback' }
  return { level: 'warn', key: 'portForward.tip.bindCustom', params: { addr: bindAddr.value } }
})

/** Tip 弹窗保持时长（ms）：出现后自动消失，不常驻页面 */
const TIP_HOLD_MS = 10_000
/** Tip 是否可见：只在页面加载、以及监听地址内容变化时短暂出现 */
const tipVisible = ref(false)
let tipTimer: number | null = null
/** 闪现一次 Tip（重置计时器）；没有提示内容时立即隐藏 */
function flashBindTip(): void {
  if (tipTimer !== null) {
    window.clearTimeout(tipTimer)
    tipTimer = null
  }
  if (!remoteBindTip.value) {
    tipVisible.value = false
    return
  }
  tipVisible.value = true
  tipTimer = window.setTimeout(() => {
    tipTimer = null
    tipVisible.value = false
  }, TIP_HOLD_MS)
}
// 页面加载（immediate）+ 服务器监听地址内容变化时闪现，10 秒后自动消失
watch(bindAddr, () => flashBindTip(), { immediate: true })
// 监听地址被修改：上一次测试得出的「不可达」红色结论随之失效，恢复默认绿色（下次测试再重新判定）
watch(bindAddr, () => { testedBlocked.value = false })

/** 服务器监听端口 Tip：修改数值（>0）时立即弹出展示，约 10 秒后自动消失（非常驻） */
const portTipVisible = ref(false)
const portTipValue = ref(0)
let portTipTimer: number | null = null
function flashPortTip(port: number | undefined): void {
  if (portTipTimer !== null) {
    window.clearTimeout(portTipTimer)
    portTipTimer = null
  }
  portTipValue.value = Number(port) || 0
  if (!(portTipValue.value > 0)) {
    portTipVisible.value = false
    return
  }
  portTipVisible.value = true
  portTipTimer = window.setTimeout(() => {
    portTipTimer = null
    portTipVisible.value = false
  }, TIP_HOLD_MS)
}

/** 端口显示文本：0 / 空 一律按「未填」显示为空；值恒被钳制到 [0, 65535] */
function portText(f: FieldDef): string {
  const n = Number(props.form?.[f.key])
  if (!Number.isFinite(n) || n <= 0) return ''
  return String(Math.min(n, 65535))
}

/**
 * 端口「强制阻止」输入：逐键即时处理，超界 / 超长字符直接丢弃不上屏。
 * - 只保留数字，且按住 5 位上限截断（65535 为最大 5 位）→ 超出的位根本输不进去；
 * - 解析后值 > 65535 立即变为 65535 → 输入 70000 实时显示成 65535；
 * 因 el-input 为受控组件，过滤后的值写回即同步刷新界面，无需等失焦。
 */
function onPortInput(f: FieldDef, raw: string): void {
  const digits = String(raw ?? '').replace(/\D/g, '').slice(0, 5)
  let n = digits ? Number(digits) : 0
  if (n > 65535) {
    n = 65535
    ElMessage.warning(t('portForward.tip.portRange'))
  }
  props.apply?.({ [f.key]: n })
  if (f.key === 'remotePort') flashPortTip(n)
}

/** 红叉之后的虚线（老路）：止于目标容器之外（动态模式容器更靠左；远程模式顶部带已上移） */
const blockedDash = computed(() => {
  const y = props.mode === 'remote' ? 164 : 192
  return props.mode === 'dynamic' ? `M755 ${y} H 920` : `M755 ${y} H 1053`
})

/**
 * 红色直线（老路）：起点为左侧设备右边缘。
 * 远程模式多出一台远程客户端时，取最右边那台的右边缘，避免起点悬空。
 */
const blockedLine = computed(() => {
  if (props.mode === 'remote') return extraClient.value ? 'M302 164 H 643' : 'M230 164 H 643'
  return 'M290 192 H 643'
})

/** 目标主机 → 目标服务 的静态短接（仅本地模式有目标服务元素） */
const LINK_PATH = 'M1175 192 H 1245'

/**
 * 连通性测试时消息的完整轨迹：隧道主链路三段
 * +（本地模式）目标主机 → 目标服务这一段，表示消息最终抵达目标服务。
 */
const TRACK_PATHS = computed(() =>
  props.mode === 'local' ? [...legPaths.value, LINK_PATH] : [...legPaths.value]
)

/**
 * 火焰轮廓：底部圆心起笔，右侧鼓起后收成偏右的尖，左侧收成内凹腰身。
 * @param cx 底部中心 x  @param baseY 底部 y  @param w 宽  @param h 高
 */
function flamePath(cx: number, baseY: number, w: number, h: number): string {
  const hw = w / 2
  const tipX = cx + hw * 0.34
  const tipY = baseY - h
  return [
    `M${cx} ${baseY}`,
    `C ${cx + hw * 1.14} ${baseY - h * 0.17}, ${cx + hw * 0.98} ${baseY - h * 0.6}, ${tipX} ${tipY}`,
    `C ${cx + hw * 0.16} ${baseY - h * 0.56}, ${cx - hw * 0.96} ${baseY - h * 0.3}, ${cx} ${baseY}`,
    'Z',
  ].join(' ')
}

/** 防火墙左上角 y：远程模式把整个墙块随顶部带上移（其余模式不变） */
const wallY = computed(() => (props.mode === 'remote' ? 76 : 100))

/** 墙砖（错缝，溢出部分由 clipPath 裁掉） */
const bricks = computed(() => {
  const out: Array<{ k: string; x: number; y: number }> = []
  for (let row = 0; row < 9; row++) {
    const offset = row % 2 === 1 ? 12 : 0
    for (let col = 0; col < 5; col++) {
      out.push({ k: `${row}-${col}`, x: 643 + offset + col * 25, y: wallY.value + 2 + row * 21 })
    }
  }
  return out
})

/**
 * 「新增远程客户端」用的伪节点 id：只用于让这台客户端在测试时独立发光，
 * 不参与链路节点判定（nodeStates 按字符串存，取不到的 id 视为 idle）。
 */
const EXTRA_CLIENT_NODE = 'clientExtra' as unknown as ForwardNodeId

/**
 * 设备：左侧容器 = 客户端（远程模式为远程客户端，本地 / 动态为本机），
 * 底部中央容器 = 中间主机 / 跳板机（SSH 服务器）。
 * 远程模式且监听地址为「自定义地址」时，中间再插一台远程客户端（上方标注 IP = 该地址），
 * 与原来那台左右并列；两台平时样式一致（绿色），只有连通性测试证实
 * 原来的那台不可达后它才转红（blocked）并持久保持。
 */
const devices = computed(() => {
  const nodes = NODES[props.mode]
  const clientLabel = t(props.compact ? nodes[0].shortKey : nodes[0].labelKey)
  const list: Array<{ key: string; x: number; y: number; node: ForwardNodeId; label: string; ip?: string; blocked?: boolean }> = [
    { key: 'left', x: clientX.value, y: props.mode === 'remote' ? 96 : 124, node: nodes[0].id, label: clientLabel, blocked: testedBlocked.value },
    { key: 'right', x: 580, y: 547, node: nodes[2].id, label: t(props.compact ? nodes[2].shortKey : nodes[2].labelKey) },
  ]
  if (extraClient.value) {
    list.splice(1, 0, {
      key: 'extra',
      x: extraClient.value.x,
      y: extraClient.value.y,
      node: EXTRA_CLIENT_NODE,
      label: clientLabel,
      ip: extraClient.value.ip,
    })
  }
  return list
})

/** 目标主机（本地 / 远程模式）：一台服务器 */
const targets = computed(() => {
  const node = NODES[props.mode][3]
  return [{ key: 'target', y: props.mode === 'remote' ? 96 : 124, label: t(props.compact ? node.shortKey : node.labelKey) }]
})

/**
 * 动态模式的三块「目标网络」图案（SVG 图标 + 名称）。
 * 坐标全部由容器尺寸反推，容器改尺寸图案自动重新居中：
 * - 水平居中：三列等宽等距，整组宽度在容器内居中；
 * - 垂直居中：整组高度在容器内居中。
 */
const netTiles = computed(() => {
  const rows = [
    { key: 'internet', icon: 'globe', label: t('portForward.net.internet') },
    { key: 'intranet', icon: 'rack', label: t('portForward.net.intranet') },
    { key: 'other', icon: 'mesh', label: t('portForward.net.other') },
  ]
  const totalW = rows.length * NET_COL_W + (rows.length - 1) * NET_COL_GAP
  const left = DYN_PANEL.value.x + (DYN_PANEL.value.w - totalW) / 2
  // 选择「配置测试目标」后输入框占顶部，图案紧贴其下，避免被输入框遮挡
  const top = props.dynTargetOpen
    ? DYN_PANEL.value.y + DYN_INPUT_H
    : DYN_PANEL.value.y + (DYN_PANEL.value.h - NET_BLOCK_H) / 2
  return rows.map((r, i) => {
    const cx = left + i * (NET_COL_W + NET_COL_GAP) + NET_COL_W / 2
    return { ...r, cx, chipX: cx - NET_CHIP / 2, chipY: top, labelY: top + NET_CHIP + 20 }
  })
})

/** 动态模式目标网络容器的发光 halo：随三块图案的位置动态生成，容器放大（出现测试目标输入框）
 *  时图案下移，halo 一并下移包住图案，避免发光区域停在旧位置 */
const dynHalo = computed(() => {
  if (props.mode !== 'dynamic' || !netTiles.value.length) return null
  const t0 = netTiles.value[0]
  const x = DYN_PANEL.value.x + (DYN_PANEL.value.w - 400) / 2
  const y = t0.chipY - 14
  const h = t0.labelY - t0.chipY + 34
  return { x, y, w: 400, h }
})

const nodeStates = reactive<Record<string, 'idle' | 'ok' | 'fail'>>({})
const legStates = reactive<Record<number, 'idle' | 'ok' | 'fail'>>({})
const packet = reactive({ visible: false, x: 0, y: 0, sim: false })
/** 第二颗光球：仅在双客户端（自定义监听地址）时出现，演示原来那台客户端的消息 ——
 *  出发时与正常消息一样是绿色，抵达服务器被拒的那一刻才转红 */
const packet2 = reactive({ visible: false, x: 0, y: 0, rejected: false, sim: false })
/** 模拟（黄色）消息的链路 / 节点 / 新增客户端连线：仅这些元素在「可达」态用黄色，
 *  其余真实请求用绿色——以此区分「真实请求」与「模拟推演」。
 *  失败（被拒）一律红色，不在此列。随 clearStates() 一起复位 */
const simLegs = ref<Set<number>>(new Set())
const simNodes = ref<Set<ForwardNodeId>>(new Set())
const simExtra = ref(false)
/** 「新增客户端 → SSH服务器」连线的结果态：测试中该链路可达（真实绿/模拟黄）或不可达（红）。
 *  这条线不在 legPaths 里，须单独记录；随 clearStates() 一起复位 */
const extraState = ref<'idle' | 'ok' | 'fail'>('idle')
/** 红色 ✕ 标记：请求被服务器拒绝后在「原客户端 → SSH服务器」链路上显示 */
const blockedMark = reactive({ visible: false, x: 212, y: 603 })
/** 第二台远程客户端（连监听地址）不可达时打的红叉，与 blockedMark 各自独立、可同屏共存 */
const extraBlockedMark = reactive({ visible: false, x: 212, y: 603 })
/** 测试结果徽标（测试结束后显示，保持一段时间后自动消失） */
const outcome = ref<{ ok: boolean; text: string } | null>(null)
/** 光球每段飞行时长（ms）。刻意等于发光脉冲周期（1s）：消息每“跳”一秒抵达下一个节点，
 *  各节点发光便始终同频、同相、步调一致；测试结束再由 clearStates() 一次性同时熄灭。 */
const TEST_LEG_MS = 1000
/** 测试序号：复位/重测时 +1，让进行中的播放自行中止 */
let testSeq = 0
/** 结果通知保持时长（ms）：到时自动消失（设备灯光保留） */
const BADGE_HOLD_MS = 6000
/** 结果通知自动消失的定时器 */
let badgeTimer: number | null = null

function nodeState(id: ForwardNodeId): 'idle' | 'ok' | 'fail' {
  return nodeStates[id] ?? 'idle'
}
function legState(i: number): 'idle' | 'ok' | 'fail' {
  return legStates[i] ?? 'idle'
}
/**
 * 某节点在架构图里要渲染的字段。
 * 「认证方式」是单选而非输入框、且独占整行，放到最前，
 * 后面的输入框刚好两两成行，不会在两列网格里留下空位。
 */
function fieldsOf(node: ForwardNodeId): FieldDef[] {
  const list = FIELDS[props.mode].filter(f => f.node === node)
  return [...list.filter(f => f.type === 'auth'), ...list.filter(f => f.type !== 'auth')]
}

/** 字段是否已有内容：决定「悬浮标签」是贴在输入框内还是上浮到边框左上角 */
function isFilled(f: FieldDef): boolean {
  const v = props.form?.[f.key]
  if (v === undefined || v === null) return false
  if (f.type === 'port') return Number(v) > 0
  return String(v).length > 0
}

/**
 * 正在编辑的字段所对应的图中元素（聚焦中）。
 * 焦点落在某个输入框 / 表单元素时：只有它映射到的 SVG 元素保持全亮，
 * 其余所有元素（含其它分组的表单）统一变暗，视线自然收拢到正在配置的那一项；
 * 焦点离开后清空，整图恢复常态（既不高亮、也不变暗）。
 * 高亮只给描边上主题色、不点呼吸光晕，与「连通性测试」的发光特效明确区分。
 */
const hotEl = ref<ForwardHotKey | null>(null)

/** 是否处于「编辑中」：决定是否开启变暗（无焦点时整图常态显示） */
const focusing = computed(() => hotEl.value !== null)

/** 当前编辑对象所属的分组（「目标服务」归入目标主机分组） */
const editingZone = computed<string | null>(() => {
  const k = hotEl.value
  if (!k) return null
  return k === 'targetService' ? 'target' : k
})

/** 某元素是否为当前编辑对象（高亮） */
function isHot(key: ForwardHotKey): boolean {
  return hotEl.value === key
}
/** 主机父容器是否属于当前编辑对象（目标服务算目标主机容器内的元素） */
function isPanelOn(node: ForwardNodeId): boolean {
  return hotEl.value === node || (node === 'target' && hotEl.value === 'targetService')
}
/** 表单引导线是否属于当前编辑对象（引导线以分组 key = 节点 id 记录） */
function isLeadOn(key: string): boolean {
  return editingZone.value === key
}

/** 聚焦到某个 SVG 元素 */
function focusEl(key: ForwardHotKey): void { hotEl.value = key }
/** 监听地址输入框是否正在编辑（光标停在里面）：用于高亮「第二台客户端」及其路径线 */
const bindAddrFocused = ref(false)

/** 聚焦某个字段：按字段声明的 hot 决定高亮哪张图（缺省按所属节点） */
function focusField(f: FieldDef): void {
  focusEl(f.hot ?? f.node)
  if (f.key === 'bindAddr') bindAddrFocused.value = true
}
function blurEl(): void {
  hotEl.value = null
  bindAddrFocused.value = false
}

/** SSH 密码明文可见性（默认隐藏） */
const passwordVisible = ref(false)

/**
 * 切换 SSH 密码明文可见性。
 * 由隐藏切换为显示前，需按用户设置的保护范围验证安全密码
 * （未开启安全密码 / 未勾选「查看会话密码」时直接放行）。
 */
async function togglePasswordVisible(): Promise<void> {
  if (!passwordVisible.value) {
    if (!(await ensureUnlocked('view_password'))) return
  }
  passwordVisible.value = !passwordVisible.value
}

// 切换认证方式后回到隐藏态，避免明文长期暴露
watch(() => props.form?.sshAuthType, () => { passwordVisible.value = false })

/**
 * 表单分组：每个分组都落在其所属的父容器面板内部、设备正上方，
 * 由引导线向下指向设备顶部。因为都在自己面板里，故与其它元素互不遮挡。
 * anchor：'down' = 从盒子底部向下指向设备顶部。
 */
const zones = computed(() => {
  if (props.compact || !props.apply) return []
  const defs = [
    // 本地主机容器内（面板 40,20,380x292；设备顶部 y=124）
    // 远程模式：设备随顶部带上移 → 目标对齐点同步上移
    { key: 'client', node: 'client' as ForwardNodeId, style: { left: '3.9%', top: '4%', width: '25%' }, anchor: 'down' as const, targetY: props.mode === 'remote' ? 96 : 124 },
    // 跳板机容器内（SSH 表单：远程模式整体放大，给到更大的显示区）
    { key: 'ssh', node: 'ssh' as ForwardNodeId, style: props.mode === 'remote' ? { left: '29%', top: '31%', width: '35%' } : { left: '30.2%', top: '34.3%', width: '31%' }, anchor: 'down' as const, targetY: 547 },
    // 目标主机容器内（远程模式：本机目标配置挪到本机服务图标右侧、纵向排布、与图标拉开间距并整体放低；箭头从表单向左指向本机服务图形）
    props.mode === 'remote'
      ? { key: 'target', node: 'target' as ForwardNodeId, style: { left: '86%', top: '13%', width: '11%' }, anchor: 'left' as const, targetY: 96 }
      : props.mode === 'dynamic'
        // 动态模式：测试目标输入框放在右上「目标网络」容器（DYN_PANEL）的红框位置，仅在选择了「配置测试目标」后显示；
        // 引导线指向下方的三块目标网络图案（箭头扎进图案顶部），而非容器顶缘
        ? { key: 'target', node: 'target' as ForwardNodeId, style: props.dynTargetOpen ? { left: '66.8%', top: '18.9%', width: '30.7%' } : { display: 'none' }, anchor: 'down' as const, targetY: netTiles.value[0]?.chipY ?? (DYN_PANEL.value.y + DYN_INPUT_H) }
        : { key: 'target', node: 'target' as ForwardNodeId, style: { left: '67.6%', top: '4%', width: '24%' }, anchor: 'down' as const, targetY: 124 },
  ]
  return defs.filter(d => fieldsOf(d.node).length > 0)
})

/* ---- 引导线：按盒子真实位置动态计算 ----
 * 盒子是 HTML、高度随内容与字号变化，用写死的 SVG 坐标会导致箭头与框脱节。
 * 这里测量每个盒子相对 SVG 的位置，换算成 viewBox 坐标后再画线。 */
const zoneEls = new Map<string, HTMLElement>()
/** key = 所属分组（节点 id），用于判断编辑某个字段时这条引导线是否保持高亮 */
const leads = ref<Array<{ d: string; head: string; key: string }>>([])
let leadRaf = 0

function setZoneEl(el: Element | null, key: string): void {
  if (el) zoneEls.set(key, el as HTMLElement)
  else zoneEls.delete(key)
  scheduleLeads()
}

/** 合并到下一帧统一测量，避免一帧内多次触发 */
function scheduleLeads(): void {
  if (leadRaf) return
  leadRaf = requestAnimationFrame(() => {
    leadRaf = 0
    computeLeads()
  })
}

function computeLeads(): void {
  const svg = svgRef.value
  if (!svg || props.compact || !props.apply) {
    leads.value = []
    return
  }
  const rect = svg.getBoundingClientRect()
  if (!rect.width || !rect.height) return
  const sx = 1400 / rect.width
  const sy = 760 / rect.height
  const out: Array<{ d: string; head: string; key: string }> = []
  for (const z of zones.value) {
    const el = zoneEls.get(z.key)
    if (!el) continue
    const b = el.getBoundingClientRect()
    // 隐藏的分组（如动态模式尚未选择「配置测试目标」）不画引导线
    if (!b.width || !b.height) continue
    const cx = ((b.left + b.right) / 2 - rect.left) * sx
    if (z.anchor === 'down') {
      const bottom = (b.bottom - rect.top) * sy
      const y1 = z.targetY - 13
      const yTip = z.targetY
      out.push({
        d: `M${cx.toFixed(1)} ${bottom.toFixed(1)} V ${y1.toFixed(1)}`,
        head: `M${cx.toFixed(1)} ${yTip.toFixed(1)} l-7 -13 h14 z`,
        key: z.key,
      })
    } else if (z.anchor === 'left') {
      // 从盒子左缘水平向左：箭头尖（左侧）扎入本机服务图形右缘，指向本机服务
      const midY = ((b.top + b.bottom) / 2 - rect.top) * sy
      const left = (b.left - rect.left) * sx
      const tipX = 1175
      const baseX = tipX + 18
      out.push({
        d: `M${left.toFixed(1)} ${midY.toFixed(1)} H ${(baseX + 1).toFixed(1)}`,
        head: `M${tipX.toFixed(1)} ${midY.toFixed(1)} l18 -9 v18 z`,
        key: z.key,
      })
    }
  }
  leads.value = out
}

let leadObserver: ResizeObserver | null = null

const svgRef = ref<SVGSVGElement | null>(null)
let svgPaths: SVGPathElement[] = []
/** 「新增远程客户端 → SSH服务器」那条独立连线（仅远程模式 + 自定义监听地址时存在） */
let extraPath: SVGPathElement | null = null

/* ===================== 动画 ===================== */

/**
 * 让某颗光球沿指定路径元素飞行。
 * @param p 光球（packet / packet2）
 * @param el 路径元素
 * @param ms 飞行时长
 * @param toFrac 只飞到路径的该比例（<1 表示中途被拦下，如地址不可达）
 */
function flyPath(
  p: { visible: boolean; x: number; y: number },
  el: SVGPathElement | null,
  ms: number,
  toFrac = 1
): Promise<void> {
  return new Promise(resolve => {
    if (!el) return resolve()
    const len = el.getTotalLength()
    const start = performance.now()
    p.visible = true
    const tick = (now: number) => {
      const k = Math.min(1, (now - start) / ms)
      const pt = el.getPointAtLength(len * toFrac * k)
      p.x = pt.x
      p.y = pt.y
      if (k < 1) requestAnimationFrame(tick)
      else resolve()
    }
    requestAnimationFrame(tick)
  })
}

/** 让主消息沿第 index 段路径飞完全程 */
function flyAlong(index: number, ms: number): Promise<void> {
  return flyPath(packet, svgPaths[index] ?? null, ms)
}

function clearStates(): void {
  ;['client', 'internet', 'ssh', 'target', EXTRA_CLIENT_NODE].forEach(id => { nodeStates[id] = 'idle' })
  for (let i = 0; i < TRACK_PATHS.value.length; i++) legStates[i] = 'idle'
  extraState.value = 'idle'
  blockedMark.visible = false
  extraBlockedMark.visible = false
  // 上次测试得出的「不可达」红色阻断态一并清除：开始新一轮测试时，
  // 所有路径与节点先恢复原本的常态颜色，结果在本次测试中重新判定
  testedBlocked.value = false
  packet2.rejected = false
  packet.sim = false
  packet2.sim = false
  simLegs.value = new Set()
  simNodes.value = new Set()
  simExtra.value = false
}

/** 重新采集路径元素（模板渲染后调用） */
function grabPaths(): void {
  const svg = svgRef.value
  if (!svg) {
    svgPaths = []
    extraPath = null
    return
  }
  svgPaths = Array.from(
    svg.querySelectorAll<SVGPathElement>('.fwd-tun__over:not(.fwd-tun__over--extra)')
  ).slice(0, TRACK_PATHS.value.length)
  // 额外客户端那条连线单独取，不参与测试轨迹
  extraPath = svg.querySelector<SVGPathElement>('.fwd-tun__over--extra')
}

/** 清除结果通知的自动消失定时器 */
function clearBadgeTimer(): void {
  if (badgeTimer !== null) {
    window.clearTimeout(badgeTimer)
    badgeTimer = null
  }
}

/** 让本次测试的结果通知在保持时长后自动消失，并一并清掉节点发光/绿色边框（序号已变则忽略） */
function scheduleBadgeHide(seq: number): void {
  clearBadgeTimer()
  badgeTimer = window.setTimeout(() => {
    badgeTimer = null
    if (seq === testSeq) {
      outcome.value = null
      clearStates()        // 发光、绿色边框与通知一起消失
      packet.visible = false
      packet2.visible = false
    }
  }, BADGE_HOLD_MS)
}

/** 复位：清空状态与结果、中止进行中的测试、隐藏光球（常态无任何动画） */
function reset(): void {
  testSeq++
  clearBadgeTimer()
  clearStates()
  outcome.value = null
  void nextTick(() => {
    grabPaths()
    packet.visible = false
    packet2.visible = false
  })
}

/**
 * 各段「消息抵达」的节点。
 * 注意：远程模式画出来的顺序是 远程客户端 → SSH服务器 → JC Shell → 本机服务，
 * 与 NODE_ORDER（客户端 → Internet → 服务器 → 目标）不同，所以要单独给出映射。
 */
const LEG_END_NODES: Record<ForwardMode, ForwardNodeId[]> = {
  local: ['client', 'internet', 'ssh', 'target'],
  dynamic: ['internet', 'ssh', 'target'],
  remote: ['ssh', 'internet', 'target'],
}

/** 第 leg 段到达的节点 */
function legEndNode(leg: number): ForwardNodeId {
  const order = LEG_END_NODES[props.mode]
  return order[leg] ?? order[order.length - 1]
}

/** 各段「消息发出」的节点（与 LEG_END_NODES 对应）：消息抵达下一节点（该段结果落定）时，
 *  发出节点也在这一刻点亮——结果出来才开始发光，出发前不亮 */
const LEG_START_NODES: Record<ForwardMode, ForwardNodeId[]> = {
  local: ['client', 'internet', 'ssh', 'target'],
  dynamic: ['client', 'internet', 'ssh'],
  remote: ['client', 'ssh', 'internet'],
}
/** 第 leg 段发出消息的节点 */
function legStartNode(leg: number): ForwardNodeId {
  const order = LEG_START_NODES[props.mode]
  return order[leg] ?? order[0]
}

/**
 * 播放连通性测试：发送消息光球，沿链路逐段飞行。
 * 消息抵达哪个设备，该设备就发光并保持（累积点亮）；
 * 失败则停在出问题的那一段，对应设备亮红，并在画面上弹出 ✕ 结果徽标。
 *
 * 起点与路径随「服务器监听地址」变化：
 * - 绑回环（127.0.0.1 / localhost）或绑服务器自身内网 IP：由 SSH 服务器本身发出绿光信号并走通；
 *   同时原来的那台远程客户端也发信号，到服务器被拒收、自身及其到服务器的链路转红（并行演示）；
 * - 绑自定义地址（即非回环、非自身 IP 的独立地址）：两台远程客户端同时发消息（都是绿色）——
 *   原来那台抵达服务器被拒后才转红，新增那台沿自己的绿色连线抵达服务器；
 * - 0.0.0.0（对所有来源开放）：由远程客户端发出，正常走完各段。
 * @returns 失败的那一段（全部通过则返回 null）
 */
async function play(
  steps: ForwardTestStep[],
  opts: {
    /** 模拟（黄色）的链路序号：这些链路「可达」时发黄光，其余真实请求发绿光 */
    simLegs?: number[]
    /** 模拟（黄色）的节点 id */
    simNodes?: ForwardNodeId[]
    /** 新增客户端连线为模拟（黄色） */
    simExtra?: boolean
    /** 未配置客户端时，将首台客户端节点标记为模拟（黄色） */
    simClient?: boolean
    /** 已废弃：SSH 服务器的可达性由「是否有请求成功抵达它」决定，
     *  不再按监听地址推演自身请求的可达性（自身被拒不影响它接收其它来源的请求） */
    sshSelfOk?: boolean
  } = {},
): Promise<ForwardTestStep | null> {
  const seq = ++testSeq
  clearStates()
  simLegs.value = new Set(opts.simLegs ?? [])
  simNodes.value = new Set(opts.simNodes ?? [])
  if (opts.simClient) simNodes.value.add(NODES[props.mode][0].id)
  // 双客户端模拟：新增的那台「同地址」客户端发出的同样是模拟请求 → 黄光
  if (opts.simExtra) simNodes.value.add(EXTRA_CLIENT_NODE)
  simExtra.value = !!opts.simExtra
  outcome.value = null
  packet.visible = false
  packet2.visible = false
  await new Promise(r => requestAnimationFrame(r))
  grabPaths()
  if (seq !== testSeq) return null

  const nodes = NODES[props.mode]
  /** 消息由 SSH 服务器本身发出：绑回环（127.0.0.1 / localhost）或绑服务器自身内网 IP 时，
   *  只有服务器本机/自身发得起请求，远程客户端连不上，故由 SSH 服务器作为信号原点 */
  const fromServer = bindIsLoopback.value || bindIsSshHost.value
  /** 双客户端场景：需要同时演示「红色被服务器拒绝」与「绿色可通行」 */
  const dual = !!customBindAddr.value && !!extraPath

  // 信号出发前不点亮任何节点：节点只有在自己相关的结果出来时才发光——
  // 消息沿链路抵达哪台设备，该设备才点亮；发出信号的节点在其消息的结果
  // （抵达被接受 / 被拒）落定那一刻才变绿 / 变黄 / 变红

  // 双客户端：原来那台客户端的消息（出发时绿色）走完整条链路、抵达 SSH 服务器后被拒绝 ——
  // 被拒的那一刻消息球与该客户端才转红，并在它那条链路上打一个红色 ✕。
  // 服务器不因此变红（另一台的请求是通的）。
  // 双客户端，或绑回环 / 服务器自身内网 IP：原来的那台远程客户端发出的信号在服务器处被拒收，
  // 它及其到服务器的链路变红；与此同时（双客户端时为新客户端、其余时为 SSH 服务器本身）的绿光正常通行。
  const blockedRun = (fromServer || dual)
    ? (async () => {
      packet2.sim = simLegs.value.has(0)
      await flyPath(packet2, svgPaths[0] ?? null, TEST_LEG_MS)
      // 请求在服务器处被拒：此刻才转红，这条链路（连同原客户端）进入持久的红色阻断态
      packet2.rejected = true
      nodeStates[nodes[0].id] = 'fail'
      // 红叉钉在「原客户端 → SSH服务器」这条链路的弧长中点上（路径随单/双客户端布局而不同，
      // 用真实路径元素取点，保证红叉始终落在路径正中间而不是悬空）
      const leg0 = svgPaths[0]
      if (leg0) {
        const mid = leg0.getPointAtLength(leg0.getTotalLength() / 2)
        blockedMark.x = mid.x
        blockedMark.y = mid.y
      }
      blockedMark.visible = true
      testedBlocked.value = true
      // 红色消息球在被拒处停留一瞬再消失（与主链路的 1s 节奏并行，不影响同频发光）
      await new Promise(r => window.setTimeout(r, 400))
      packet2.visible = false
      packet2.rejected = false
    })()
    : null

  let failed: ForwardTestStep | null = null
  let extraDone = false
  for (const step of steps) {
    // 第 0 段（客户端 → 服务器）不由主消息走：绑回环时它根本发不出去；
    // 双客户端时由第二颗光球（抵达后被拒绝）负责演示。但配置错误仍要体现在结果里。
    if ((fromServer || dual) && step.leg === 0) {
      if (!step.ok) {
        legStates[0] = 'fail'
        // 第 0 段发不出去：发送消息的那台远程客户端此刻才亮红灯。
        // 但 SSH 服务器自身发出的请求是可以抵达监听端口的（绑回环 / 自身 IP 才会走到这），
        // 服务器仍然可达 → 绿光
        nodeStates[nodes[0].id] = 'fail'
        nodeStates[nodes[2].id] = 'ok'
        failed = step
        break
      }
      continue
    }

    // 双客户端：主消息先沿「新增客户端 → SSH服务器」那条连线抵达服务器并点亮它（颜色按 simExtra）——
    // 只要有一台客户端的请求能抵达，服务器就是通的。
    if (dual && !extraDone) {
      extraDone = true
      packet.sim = simExtra.value
      await flyPath(packet, extraPath, TEST_LEG_MS)
      if (seq !== testSeq) return null
      // 发出消息的新增客户端此刻才知道结果：点亮（模拟黄 / 真实绿）
      nodeStates[EXTRA_CLIENT_NODE] = 'ok'
      // SSH 服务器的可达性 = 有请求成功抵达它：B 的请求被接受即视为可达（绿光），
      // 与它自身发出的请求是否被监听地址拒绝无关（那不是「不可达」）
      nodeStates[nodes[2].id] = 'ok'
      // 这条链路同样测试过了：可达。模拟时随 is-sim 变黄，真实时为绿
      extraState.value = 'ok'
    }

    if (seq !== testSeq) return null
    // 光球颜色 = 该段消息类型（真实绿 / 模拟黄）
    packet.sim = simLegs.value.has(step.leg)
    await flyAlong(step.leg, TEST_LEG_MS)
    if (seq !== testSeq) return null
    const endNode = legEndNode(step.leg)
    if (!step.ok) {
      // 故障段：变红并让该设备亮红，停止继续发送（无论真实/模拟，失败都是红）
      legStates[step.leg] = 'fail'
      nodeStates[endNode] = 'fail'
      failed = step
      break
    }
    // 抵达设备：该段点亮（真实绿 / 模拟黄）、设备发光并保持；
    // 发出消息的节点此刻也知道了自己的结果，随之点亮
    legStates[step.leg] = 'ok'
    nodeStates[endNode] = 'ok'
    nodeStates[legStartNode(step.leg)] = 'ok'
  }

  if (blockedRun) await blockedRun
  packet.visible = false
  packet2.visible = false
  outcome.value = failed
    ? { ok: false, text: `${t('portForward.test.failed')}：${t(failed.reason)}` }
    : { ok: true, text: t('portForward.test.passed') }
  // 本次测试证实「原远程客户端 → SSH服务器」不通（绑回环等场景）：
  // 红色阻断态从此持久保持（不随发光熄灭而复原），直到页面关闭或用户修改监听地址
  if (remoteClientBlocked.value) testedBlocked.value = true
  // 结果通知保持一段时间后自动消失（设备灯光仍保留）
  scheduleBadgeHide(seq)
  return failed
}

/** 单台远程客户端真实探测的结论（由前端根据后端 remote_client_probe 结果归类） */
type RemoteProbeState = 'ok' | 'sshFail' | 'blocked'

/**
 * 远程模式「已配置两台远程客户端」时的真实连通性测试动画。
 * 这一条不走拓扑推导，直接用后端真实探测结果驱动：
 * - left（连接非监听地址）：sshFail 直接红框（且无法发请求）；否则绿光出发、到服务器被拒、转红 ✕（预期被挡）；
 * - right（连接监听地址）：ok 则沿自己的绿色连线抵达服务器、服务器与后续各段点亮；sshFail 直接红框；
 * 信号一律绿色（真实请求），与「未配置时的模拟（黄色）」区分。
 * @returns 整体是否通过（right 可达即视为隧道可用）
 */
async function playRemoteReal(
  left: RemoteProbeState,
  right: RemoteProbeState,
): Promise<boolean> {
  const seq = ++testSeq
  clearStates()
  outcome.value = null
  packet.visible = false
  packet2.visible = false
  await new Promise(r => requestAnimationFrame(r))
  grabPaths()
  if (seq !== testSeq) return false

  const nodes = NODES[props.mode]
  const leftId = nodes[0].id
  let ok = false

  // 左右两台客户端「同时」发出真实请求：互不等待，各自在抵达点判定结果
  const leftRun = (async () => {
    if (seq !== testSeq) return
    if (left === 'sshFail') {
      nodeStates[leftId] = 'fail'
      legStates[0] = 'fail'
      testedBlocked.value = true
      return
    }
    // 出发前不点亮：抵达服务器被拒的那一刻才知道结果，随即转红
    packet2.sim = false
    await flyPath(packet2, svgPaths[0] ?? null, TEST_LEG_MS)
    if (seq !== testSeq) return
    packet2.rejected = true
    nodeStates[leftId] = 'fail'
    const leg0 = svgPaths[0]
    if (leg0) {
      const mid = leg0.getPointAtLength(leg0.getTotalLength() / 2)
      blockedMark.x = mid.x
      blockedMark.y = mid.y
    }
    blockedMark.visible = true
    testedBlocked.value = true
    packet2.visible = false
    packet2.rejected = false
  })()

  const rightRun = (async () => {
    if (seq !== testSeq) return
    if (right === 'sshFail') {
      nodeStates[EXTRA_CLIENT_NODE] = 'fail'
      extraState.value = 'fail'
      return
    }
    if (right === 'ok') {
      packet.sim = false
      if (extraPath) {
        await flyPath(packet, extraPath, TEST_LEG_MS)
        if (seq !== testSeq) return
      }
      // 真实测试证实这条链路可达：右客户端随信号抵达一同点亮（所有消息都是真实请求，无模拟黄）
      nodeStates[EXTRA_CLIENT_NODE] = 'ok'
      extraState.value = 'ok'
      return
    }
    // 能认证但监听端口不可达：请求沿右侧连线发出，途中被拒，在其链路上打 ✕（与左侧并行、各自独立）
    packet.sim = false
    if (extraPath) {
      await flyPath(packet, extraPath, TEST_LEG_MS)
      if (seq !== testSeq) return
    }
    nodeStates[EXTRA_CLIENT_NODE] = 'fail'
    extraState.value = 'fail'
    if (extraPath) {
      const mid = extraPath.getPointAtLength(extraPath.getTotalLength() / 2)
      extraBlockedMark.x = mid.x
      extraBlockedMark.y = mid.y
    }
    extraBlockedMark.visible = true
    packet.visible = false
  })()

  await Promise.all([leftRun, rightRun])
  if (seq !== testSeq) return false

  // 右侧可达时，继续演示：右客户端抵达服务器后，服务器 → JC Shell → 本机服务 各段依次点亮
  if (right === 'ok') {
    for (const step of testStepsForRemote()) {
      if (seq !== testSeq) return false
      packet.sim = false
      await flyAlong(step.leg, TEST_LEG_MS)
      if (seq !== testSeq) return false
      const endNode = legEndNode(step.leg)
      if (!step.ok) {
        legStates[step.leg] = 'fail'
        nodeStates[endNode] = 'fail'
        break
      }
      legStates[step.leg] = 'ok'
      nodeStates[endNode] = 'ok'
      nodeStates[legStartNode(step.leg)] = 'ok'
    }
    ok = true
  }

  // SSH 服务器的可达性 = 有请求成功抵达它（B 的真实请求 / 它自身的请求），一律绿光
  nodeStates[nodes[2].id] = 'ok'

  scheduleBadgeHide(seq)
  return ok
}

/** 远程模式真实测试时，服务器 → JC Shell → 本机服务 这几段（跳过客户端那段） */
function testStepsForRemote(): ForwardTestStep[] {
  return (props.form ? buildTestSteps(props.form) : []).filter(s => s.leg !== 0)
}

// 表单被编辑后，上次测试结果即失效：清掉发光与结果通知（配置变了，旧结论不再成立）
watch(() => props.form, () => {
  clearBadgeTimer()
  clearStates()
  outcome.value = null
  packet.visible = false
  packet2.visible = false
}, { deep: true })

onMounted(() => {
  void nextTick(() => {
    grabPaths()
    scheduleLeads()
  })
  // 引导线起点依赖盒子尺寸：窗口/弹窗尺寸变化时重新测量
  window.addEventListener('resize', scheduleLeads)
  const canvas = svgRef.value?.parentElement
  if (typeof ResizeObserver !== 'undefined' && canvas) {
    leadObserver = new ResizeObserver(() => scheduleLeads())
    leadObserver.observe(canvas)
  }
})

onBeforeUnmount(() => {
  testSeq++          // 中止进行中的测试
  clearBadgeTimer()
  if (tipTimer !== null) {   // 清掉 Tip 的自动消失定时器
    window.clearTimeout(tipTimer)
    tipTimer = null
  }
  packet.visible = false
  packet2.visible = false
  window.removeEventListener('resize', scheduleLeads)
  leadObserver?.disconnect()
  leadObserver = null
  if (leadRaf) cancelAnimationFrame(leadRaf)
})

/** 选择私钥文件（仅 Tauri 环境可用；托管私钥内容，连接时由后端落盘即可认证） */
async function pickKeyFile(): Promise<void> {
  if (!isTauriEnv()) return
  try {
    const path = await invoke<string | null>('pick_key_file')
    if (!path) return
    const data = await invoke<{ content: string; name: string }>('read_key_file', { path })
    props.apply?.({ sshKeyPath: path, sshKeyContent: data.content, sshKeyName: data.name || path.split(/[\\/]/).pop() || '' })
  } catch (e) {
    console.warn('[portForward] 读取私钥文件失败:', e)
  }
}

defineExpose({ play, reset, playRemoteReal })
</script>

<style scoped>
.fwd {
  width: 100%;
}
.fwd-canvas {
  position: relative;
  width: 100%;
  /* 一屏显示：弹窗已铺满主窗口（100vh），减去弹窗头/脚等固定占位（约 140px）后，
     按画布比例（1400×760 ≈ 1.842）反推最大宽度并居中；
     宽屏时该上限大于容器宽度，于是自动铺满整宽（左右只留弹窗自身的内边距）。 */
  max-width: calc((100vh - 175px) * 1.842);
  margin: 0 auto;
}
.fwd-svg {
  display: block;
  width: 100%;
  height: auto;
}

/* ===== 编辑聚焦：正在编辑某个字段时，只有它映射到的元素保持全亮，其余元素统一变暗 =====
   .fwd-el 标记「参与明暗切换」的元素，.is-on 表示它就是当前编辑对象；
   没编辑任何字段时（.fwd-svg 上没有 .is-dim）整图保持常态、既不亮也不暗。 */
.fwd-el {
  transition: opacity 0.28s ease;
}
.fwd-svg.is-dim .fwd-el {
  opacity: 0.2;
}
.fwd-svg.is-dim .fwd-el.is-on {
  opacity: 1;
}
/* 「编辑中」高亮的第二台客户端保持全亮：正在填监听地址时（编辑对象是 SSH 服务器分组，
   其余元素变暗），它和它的路径线不吃这个变暗，始终醒目提示「当前填的 IP 就是这台客户端」 */
.fwd-svg.is-dim .fwd-el.is-editing {
  opacity: 1 !important;
}

/* ===== 主机父容器面板：把设备、配置表单、服务元素圈在一起 ===== */
.fwd-panel {
  fill: #262e42;
  fill-opacity: 0.72;
  stroke: #3b4763;
  stroke-width: 1.5;
  transition: stroke 0.25s ease, fill-opacity 0.25s ease, opacity 0.28s ease;
}
.fwd-panel.is-ok { stroke: #4ade80; stroke-opacity: 0.75; }
.fwd-panel.is-fail { stroke: #ef4444; stroke-opacity: 0.75; }
.fwd-panel.is-hot { stroke: var(--jc-accent); }
/* 远程模式：连通性测试证实客户端不可达后，其整块面板持久保持变暗
   （直到页面关闭或用户修改服务器监听地址，不随发光效果结束而复原） */
.fwd-panel.is-blocked { opacity: 0.35; }
/* 正在配置某台远程客户端时，它所在的面板保持全亮：
   否则面板被 is-blocked 压到 0.35，会让高亮节点看起来「陷在暗底里」 */
.fwd-panel.is-editing { opacity: 1 !important; }

/* ===== 红色「被挡住」的老路 ===== */
.fwd-blk__line {
  fill: none;
  stroke: #ef4444;
  stroke-width: 6;
  stroke-linecap: round;
}
.fwd-blk__line--dashed {
  stroke-width: 4;
  stroke-dasharray: 8 8;
  opacity: 0.5;
}

/* ===== 引导线：表单区块 → 元素 ===== */
.fwd-lead {
  stroke: #5a6787;
  stroke-width: 2;
  stroke-dasharray: 5 5;
  fill: none;
}
.fwd-lead__head {
  fill: #5a6787;
}

/* ===== 火焰：底部为原点轻微跳动 ===== */
.fwd-flame {
  transform-box: fill-box;
  transform-origin: 50% 100%;
  animation: fwd-flicker 1.9s ease-in-out infinite;
}
@keyframes fwd-flicker {
  0%, 100% { transform: scaleY(1) scaleX(1); }
  34% { transform: scaleY(1.06) scaleX(0.98); }
  68% { transform: scaleY(0.96) scaleX(1.02); }
}

/* ===== 绿色隧道：虚线常驻流动，表示消息在隧道里跑 ===== */
.fwd-tun__under {
  fill: none;
  stroke: #1b3b33;
  stroke-width: 15;
  stroke-linecap: round;
  transition: stroke 0.25s ease;
}
.fwd-tun__over {
  fill: none;
  /* 用纯色而不是渐变：竖直/水平直线的包围盒在某一维为 0，
     objectBoundingBox 渐变会整段不渲染（拉直后亮绿虚线会莫名消失）。
     纯色与几何无关，任何走向都稳定。ok/fail 时下面的类规则会覆盖它。 */
  stroke: #34d399;
  stroke-width: 6;
  stroke-linecap: round;
  stroke-dasharray: 18 12;
  animation: fwd-flow 0.9s linear infinite;
  transition: stroke 0.25s ease;
}
@keyframes fwd-flow {
  to { stroke-dashoffset: -30; }
}
/* ===== 隧道段（远程模式：SSH服务器 ↔ JC Shell）：画成一根管道 =====
   rim = 管壁外圈描边（在深色底上勾出管道轮廓）；under = 管壁；over = 中心亮线（消息流）。
   不再加环肋，避免看成「蠕动的虫子」。
   注意：本块要放在下面的 is-ok / is-fail 规则之前，让测试结果的颜色能覆盖默认色。 */
.fwd-tun.is-tunnel .fwd-tun__rim {
  fill: none;
  stroke: #4e8b7d;
  stroke-width: 32;
  /* 与管壁统一用圆头收口：端口是闭合的圆弧，不会露出「没封死」的开口 */
  stroke-linecap: round;
  opacity: 0.85;
  transition: stroke 0.25s ease;
}
.fwd-tun.is-tunnel .fwd-tun__under {
  stroke: #17493f;
  stroke-width: 26;
  stroke-linecap: round;
  opacity: 1;
}
.fwd-tun.is-tunnel .fwd-tun__over {
  stroke: #9dffcf;
  stroke-width: 4;
}
/* 测试结果：外描边跟着一起变绿 / 变红，保持整段隧道一致 */
.fwd-tun.is-tunnel.is-ok .fwd-tun__rim { stroke: #4ade80; }
.fwd-tun.is-tunnel.is-fail .fwd-tun__rim { stroke: #8b3a37; }
/* 隧道文字标注（「SSH 隧道」） */
.fwd-tun__label {
  fill: #cdd6ea;
  font-size: 15px;
}
.fwd-tun.is-ok .fwd-tun__over { stroke: #4ade80; }
.fwd-tun.is-fail .fwd-tun__under { stroke: #5b1f22; }
.fwd-tun.is-fail .fwd-tun__over { stroke: #ef4444; animation: none; }
/* 远程模式：连通性测试证实「远程客户端 → SSH服务器」不通后，这一段持久保持阻断态：
   整体变暗、颜色由绿转红并停止流动，直到页面关闭或监听地址被修改。
   测试前/测试通过时与其它链路一样是默认的绿色流动。 */
.fwd-tun.is-blocked { opacity: 0.55; }
.fwd-tun.is-blocked .fwd-tun__under { stroke: #5b1f22; }
.fwd-tun.is-blocked .fwd-tun__over { stroke: #ef4444; animation: none; }
/* 「目标主机 → 目标服务」的直连：实线、不流动（与隧道主链路的流动虚线区分），
   但保持足够可见度 —— 只留深绿底衬会因与面板底色接近而像「消失」。
   上面的 is-ok / is-fail 规则优先级更高，测试时会正常点亮。 */
.fwd-tun__over--link {
  stroke: #34d399;
  stroke-dasharray: none;
  animation: none;
}

/* ===== 设备 ===== */
.fwd-dev__box {
  stroke: #8fa0bd;
  stroke-opacity: 0.35;
  stroke-width: 2;
  transition: stroke 0.25s ease, stroke-opacity 0.25s ease;
}
.fwd-dev__led { fill: #35d07f; transition: fill 0.25s ease; }
.fwd-dev.is-ok .fwd-dev__box { stroke: #4ade80; stroke-opacity: 1; }
.fwd-dev.is-fail .fwd-dev__box { stroke: #ef4444; stroke-opacity: 1; }
.fwd-dev.is-fail .fwd-dev__led { fill: #ef4444; }
/* 远程模式：连通性测试证实原来的远程客户端不可达后：
   保持变暗 + 红框 + 红灯（持久），表示它的请求到不了 SSH 服务器；
   红色光晕只在测试动画期间出现，随发光结束一起熄灭 */
.fwd-dev.is-blocked { opacity: 0.55; }
.fwd-dev.is-blocked .fwd-dev__box { stroke: #ef4444; stroke-opacity: 1; }
.fwd-dev.is-blocked .fwd-dev__led { fill: #ef4444; }
/* 额外那台远程客户端的 IP（标在设备上方） */
.fwd-dev__ip {
  fill: #86efac;
  font-size: 14px;
  font-weight: 600;
}
/* 第二台客户端「正在被编辑」：光标还停在监听地址框里时，用它独有的琥珀色把设备和路径线点亮，
   提示当前正在填的 IP 就是这台客户端（区别于 1Hz 测试呼吸光与常规焦点高亮） */
.fwd-dev.is-editing .fwd-dev__box { stroke: #fbbf24; stroke-opacity: 1; stroke-width: 3; }
.fwd-dev.is-editing .fwd-dev__led { fill: #fbbf24; }
.fwd-dev.is-editing .fwd-dev__ip { fill: #fbbf24; }
.fwd-tun.is-editing .fwd-tun__under { stroke: #5c4a1a; }
/* 高亮链路同样保持流动：is-blocked / is-fail 会把 animation 设为 none，这里恢复 */
.fwd-tun.is-editing .fwd-tun__over { stroke: #fbbf24; animation: fwd-flow 0.9s linear infinite; }
/* 高亮期间强制全亮（!important 兜底）：无论此前是「不可达」阻断态还是整图编辑变暗，
   正在配置的这台客户端都必须完整显现，让黄色描边与链路凸显出来 */
.fwd-dev.is-editing { opacity: 1 !important; }
.fwd-tun.is-editing { opacity: 1 !important; }

/* ===== 远程客户端「IP + 编辑」标签（两台都配置好后显示在各自节点上方） ===== */
.fwd-ctag {
  position: absolute;
  z-index: 3;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 9px;
  border-radius: 999px;
  background: #3a3410;
  border: 1px solid #fbbf24;
  color: #fbbf24;
  font-size: 13px;
  font-weight: 600;
  line-height: 1.4;
  white-space: nowrap;
  /* 以节点上方中心为锚点：水平居中、底边贴在节点顶部之上 */
  transform: translate(-50%, -100%);
  box-shadow: 0 4px 12px -6px rgba(0, 0, 0, 0.85);
}
.fwd-ctag__edit {
  cursor: pointer;
  font-size: 14px;
  color: #fbbf24;
  transition: color 0.15s ease;
}
.fwd-ctag__edit:hover { color: #fff; }
.fwd-ctag__ip { color: #fde68a; }

/* ===== JC Shell 节点（原「云」）===== */
.fwd-cloud__box {
  stroke: #3d4a6b;
  stroke-width: 2;
  transition: stroke 0.25s ease;
}
.fwd-cloud.is-ok .fwd-cloud__box { stroke: #4ade80; }
.fwd-cloud.is-fail .fwd-cloud__box { stroke: #ef4444; }
.fwd-cloud.is-highlight .fwd-cloud__box { stroke: var(--jc-accent); }

/* ===== 目标 ===== */
/* 默认与其它设备一致：淡灰描边（绿色只在测试通过时点亮） */
.fwd-target__box {
  stroke: #8fa0bd;
  stroke-opacity: 0.35;
  stroke-width: 2;
  transition: stroke 0.25s ease, stroke-opacity 0.25s ease;
}
.fwd-target.is-ok .fwd-target__box { stroke: #4ade80; stroke-opacity: 1; }
.fwd-target.is-fail .fwd-target__box { stroke: #ef4444; stroke-opacity: 1; }
.fwd-target.is-highlight .fwd-target__box { stroke: var(--jc-accent); stroke-opacity: 1; }
.fwd-target__led { fill: #35d07f; transition: fill 0.25s ease; }
.fwd-target.is-fail .fwd-target__led { fill: #ef4444; }

/* ===== 目标服务（目标主机旁的独立小框）===== */
.fwd-tgtsvc__box {
  transition: stroke 0.25s ease, background-color 0.25s ease;
}
.fwd-tgtsvc__text {
  fill: #7dffb0;
  font-size: 13px;
}

/* ===== 动态模式：目标网络图案（SVG 图标 + 名称）===== */
.fwd-net__chip {
  fill: #22304e;
  stroke: #3d5a8a;
  stroke-width: 1.5;
}
/* 图案用细描边而不是字形/位图：随画布缩放始终锐利，也不会受字体缺失影响 */
.fwd-net__stroke {
  fill: none;
  stroke: #6fc9ff;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.fwd-net__dot { fill: #6fc9ff; }
.fwd-net__text {
  fill: #cdd6ea;
  font-size: 15px;
}
/* 紧凑模式（模式卡片 / 向导）里画布只有 200~500px 宽，三列并排的名称在这个尺度下
   无论怎么放大都会互相压住，故只保留三块图案（图标本身已足以表意） */
.fwd--compact .fwd-net__text { display: none; }

.fwd-cap,
.fwd-target__text {
  fill: #cdd6ea;
  font-size: 15px;
}
/* 紧凑模式（模式卡片 / 向导）：限宽并把字放大，保证缩小后标签仍看得清；
   向导里图示下方还有步骤标题/说明/表单，故高度预留更多，依旧保证一屏放下 */
.fwd--compact .fwd-canvas {
  max-width: min(520px, calc((100vh - 380px) * 1.842));
  margin: 0 auto;
}
.fwd--compact .fwd-cap,
.fwd--compact .fwd-target__text { font-size: 34px; font-weight: 600; }

/* ===== 表单：贴在对应元素旁边 ===== */
.fwd-zone {
  position: absolute;
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px 10px;
  padding: 7px;
  border-radius: 10px;
  border: 1px solid var(--jc-border);
  /* 不透明：悬浮标签需要用它做底，才能干净地「截断」输入框上边框 */
  background: var(--jc-bg-panel);
  box-shadow: 0 8px 20px -12px rgba(0, 0, 0, 0.75);
  /* 编辑某个字段时，其它分组整体变暗：用 brightness 而不是 opacity，
     分组底色保持不透明，不会透视出后面的 SVG 图形而显得脏 */
  transition: filter 0.28s ease;
}
/* 非当前编辑对象的分组 */
.fwd-zone.is-dim {
  filter: brightness(0.55) saturate(0.75);
}
/* 远程模式的目标配置：单列纵向排布（本机目标主机在上、本机目标端口在下） */
.fwd-zone--col {
  grid-template-columns: 1fr;
}
/* 动态模式测试目标表单：右侧留出关闭按钮的竖向通道，避免按钮压住端口输入框 */
.fwd-zone--dyn-close {
  padding-right: 34px;
}
/* 动态模式测试目标表单的关闭按钮（右上角垂直居中，非必填可关闭） */
.fwd-zone__close {
  position: absolute;
  top: 50%;
  right: 8px;
  transform: translateY(-50%);
  z-index: 2;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: none;
  border-radius: 7px;
  background: transparent;
  color: var(--jc-text-muted);
  cursor: pointer;
  transition: background 0.18s ease, color 0.18s ease;
}
.fwd-zone__close:hover {
  background: color-mix(in srgb, var(--jc-accent) 16%, transparent);
  color: var(--jc-accent);
}
.fwd-zone__close .el-icon {
  font-size: 16px;
}
/* 分组标题独占整行，字段再两两排布 */
.fwd-zone .fwd-zone__head {
  grid-column: 1 / -1;
}
.fwd-field {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}
/* 非输入框说明文案（如「认证方式」）：独占一行 */
.fwd-field__caption {
  font-size: 12px;
  line-height: 1.3;
  color: var(--jc-text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
/* 输入框：加高 + 加大字号；底色与面板一致，悬浮标签上浮后底色才能无缝衔接 */
.fwd-field :deep(.el-input__wrapper) {
  padding: 7px 12px;
  background-color: var(--jc-bg-panel) !important;
}
.fwd-field :deep(.el-input__inner) {
  height: 22px;
  line-height: 22px;
  font-size: 14px;
  /* 光标用主题色：深色底上默认光标太暗，几乎看不到 */
  caret-color: var(--jc-accent);
}
.fwd-field :deep(.el-input-number) {
  width: 100%;
}
/* 服务器监听端口 Tip：内联在输入框正下方（文档流占位，不与其它提示重叠），
   深色底上用暖色警示配色保证可读 */
.fwd-port-tip {
  /* 横跨分组网格整行：宽度与表单一致 */
  grid-column: 1 / -1;
  display: flex;
  align-items: flex-start;
  gap: 6px;
  margin-top: 2px;
  padding: 5px 8px;
  border-radius: 8px;
  border: 1px solid rgba(245, 158, 11, 0.5);
  background: color-mix(in srgb, #f59e0b 14%, var(--jc-bg-panel, #1d2334));
  color: #fcd34d;
  font-size: 11px;
  line-height: 1.45;
}
.fwd-port-tip__icon {
  flex: 0 0 auto;
}
/* 密码可见性切换的小眼睛 */
.fwd-eye {
  cursor: pointer;
  color: var(--jc-text-muted);
  transition: color 0.15s ease;
}
.fwd-eye:hover {
  color: var(--jc-accent);
}

/* ===== 悬浮标签：空值时贴在输入框内，有内容后上浮到边框左上角（描边输入框样式） ===== */
.fwd-field__label {
  position: absolute;
  left: 11px;
  top: 50%;
  z-index: 2;
  transform: translateY(-50%);
  padding: 0 4px;
  font-size: 14px;
  line-height: 1.2;
  color: var(--jc-text-muted);
  background: var(--jc-bg-panel);
  pointer-events: none;
  white-space: nowrap;
  transition: top 0.16s ease, font-size 0.16s ease, color 0.16s ease;
}
/* 有内容、或输入框获得焦点（光标已就位）时上浮，
   否则标签会盖在光标上，看起来像「光标被 placeholder 挡住」 */
.fwd-field--float.is-float .fwd-field__label,
.fwd-field--float:focus-within .fwd-field__label {
  top: 0;
  font-size: 12px;
  color: var(--jc-accent);
}

/* SSH 分组标题 + 选择会话按钮 */
.fwd-zone__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  margin-bottom: 2px;
}
.fwd-zone__title {
  font-size: 11px;
  font-weight: 600;
  color: var(--jc-text);
}
.fwd-pick {
  flex-shrink: 0;
  padding: 6px 14px;
  height: 30px;
  font-size: 13px;
}
.fwd-pick .el-icon {
  margin-right: 4px;
}
/* 认证方式：说明文案独占一行，单选按钮独占下一行的整行宽度 */
.fwd-field--auth {
  grid-column: 1 / -1;
}
.fwd-field--auth :deep(.el-radio-group) {
  display: flex;
  width: 100%;
}
.fwd-field--auth :deep(.el-radio-button) {
  flex: 1;
}
.fwd-field--auth :deep(.el-radio-button__inner) {
  width: 100%;
  padding: 6px 10px;
  font-size: 14px;
}
/* 私钥：输入框 + 浏览按钮一行（高度与其它输入框一致） */
.fwd-field--key .fwd-key-row {
  display: flex;
  gap: 8px;
}
.fwd-field--key .fwd-key-row .el-input {
  flex: 1;
  min-width: 0;
}
.fwd-field--key .fwd-key-row .el-button {
  height: 36px;
}

/* ===== 表单下方的 Tip 弹窗（如「服务器监听地址」取值提示）=====
   绝对定位挂在分组下方（浮层，不占布局）：只在页面加载 / 该输入框内容变化时短暂出现 */
.fwd-tip {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  right: 0;
  z-index: 6;
  display: flex;
  align-items: flex-start;
  gap: 6px;
  padding: 6px 9px;
  border-radius: 8px;
  border: 1px solid transparent;
  font-size: 12px;
  line-height: 1.45;
  box-shadow: 0 10px 22px -14px rgba(0, 0, 0, 0.85);
  pointer-events: none;
}
.fwd-tip-fade-enter-active,
.fwd-tip-fade-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}
.fwd-tip-fade-enter-from,
.fwd-tip-fade-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
.fwd-tip.is-warn {
  color: #fcd34d;
  background: color-mix(in srgb, #f59e0b 14%, var(--jc-bg-panel, #1d2334));
  border-color: rgba(245, 158, 11, 0.5);
}
.fwd-tip.is-ok {
  color: #86efac;
  background: color-mix(in srgb, #4ade80 14%, var(--jc-bg-panel, #1d2334));
  border-color: rgba(74, 222, 128, 0.5);
}
.fwd-tip__icon {
  flex: 0 0 auto;
}

/* ===================== 连通性测试：消息光球 + 设备点亮并保持 ===================== */
/* 消息光球：仅在点击测试时出现（常态无任何动画） */
.fwd-packet { fill: #7dffb0; opacity: 1; }
.fwd-packet-halo {
  fill: none;
  stroke: #7dffb0;
  stroke-width: 2;
  transform-box: fill-box;
  transform-origin: center;
  animation: fwd-halo-pulse .7s ease-out infinite;
}
/* 双客户端时那颗「被拦下」的红色光球 */
.fwd-packet.is-blocked { fill: #ef4444; }
.fwd-packet-halo.is-blocked { stroke: #ef4444; }
/* 模拟（黄色）消息：被标记为 is-sim 的链路「可达」态、节点绿光、光球统一用黄色，
   与真实请求的绿色区分。失败（is-fail）一律红色，不受此影响 */
.fwd-packet.is-sim { fill: #fbbf24; }
.fwd-packet-halo.is-sim { stroke: #fbbf24; }
.fwd-tun.is-ok.is-sim .fwd-tun__over { stroke: #fbbf24; }
/* is-sim 挂在节点容器 <g>（fwd-dev / fwd-cloud / fwd-target）上，光晕是其子元素，
   需用后代选择器才能命中——否则模拟黄光下光晕仍是默认绿色 */
g.is-ok.is-sim .fwd-node-halo { stroke: #fbbf24; }
.fwd-dev.is-ok.is-sim .fwd-dev__box { stroke: #fbbf24; stroke-opacity: 1; }
.fwd-dev.is-ok.is-sim .fwd-dev__led { fill: #fbbf24; }
/* 失败（被拒）光球无论模拟/真实都是红色，不被模拟黄覆盖 */
.fwd-packet.is-blocked { fill: #ef4444; }
.fwd-packet-halo.is-blocked { stroke: #ef4444; }
@keyframes fwd-halo-pulse {
  0% { transform: scale(.35); opacity: .95; }
  100% { transform: scale(1.7); opacity: 0; }
}

/* 填表时的高亮（焦点落在对应输入框）：只把「SVG 图形本身」的描边换成主题色并略微加粗，
   不加呼吸光晕，与「连通性测试」的发光特效区分开；外层父容器不参与高亮。 */
.fwd-dev.is-highlight .fwd-dev__box,
.fwd-target__box.is-hot {
  stroke: var(--jc-accent);
  stroke-opacity: 1;
  stroke-width: 3;
}
.fwd-tgtsvc__box.is-hot {
  stroke: var(--jc-accent);
  stroke-width: 2.5;
}

/* 节点发光：盒体后面叠一圈同形状的亮色描边，经 SVG 模糊滤镜(fwdNodeGlow)扩散成光晕，
   与绿色/红色边框一起呈现"发光"效果（比单纯描边或 drop-shadow 对深色盒子有效得多） */
.fwd-node-halo {
  fill: none;
  stroke-width: 8;
  pointer-events: none;
  /* 所有节点统一 1Hz 呼吸：ok / fail 同频率、同节奏（脉冲同时起落） */
  animation: fwd-halo-breath 1s ease-in-out infinite;
}
.fwd-node-halo.is-ok { stroke: #4ade80; }
.fwd-node-halo.is-fail { stroke: #ff5a5a; }
@keyframes fwd-halo-breath {
  0%, 100% { opacity: .5; }
  50% { opacity: 1; }
}
.fwd-dev.is-ok .fwd-dev__led { fill: #4ade80; }

/* 测试结果徽标：叠加在画面顶部正中，测试结束弹出 */
.fwd-badge {
  position: absolute;
  left: 50%;
  top: 12px;
  transform: translateX(-50%);
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 7px 16px;
  border-radius: 999px;
  font-size: 13px;
  font-weight: 600;
  white-space: nowrap;
  z-index: 6;
  pointer-events: none;
  box-shadow: 0 8px 24px -12px rgba(0, 0, 0, .8);
}
.fwd-badge.is-ok {
  color: #86efac;
  background: color-mix(in srgb, #4ade80 18%, var(--jc-bg-panel, #1d2334));
  border: 1px solid rgba(74, 222, 128, .55);
}
.fwd-badge.is-fail {
  color: #fca5a5;
  background: color-mix(in srgb, #ef4444 18%, var(--jc-bg-panel, #1d2334));
  border: 1px solid rgba(239, 68, 68, .55);
}
.fwd-badge__icon { font-size: 15px; line-height: 1; }
.fwd-badge-pop-enter-active,
.fwd-badge-pop-leave-active { transition: opacity .22s ease, transform .22s ease; }
.fwd-badge-pop-enter-from,
.fwd-badge-pop-leave-to { opacity: 0; transform: translateX(-50%) translateY(-6px); }
</style>
