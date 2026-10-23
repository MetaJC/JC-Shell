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
  <!--
    终端会话图标（窗口 + 「>_」提示符）
    作者：JC

    会话列表里代表「一个终端会话」，比显示器/加号等图标语义更准确。
    用 currentColor 描边，颜色随外层文字色（选中态可自行覆盖）。
    尺寸用 em：外层设置 font-size 即可缩放。
  -->
  <svg
    viewBox="0 0 16 16"
    width="1em"
    height="1em"
    fill="none"
    stroke="currentColor"
    stroke-width="1.3"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <!-- 终端窗口 -->
    <rect x="1.2" y="2.3" width="13.6" height="11.4" rx="2.4" />
    <!-- 提示符「>」 -->
    <polyline points="4.7,6.3 6.6,8.1 4.7,9.9" />
    <!-- 下划线「_」 -->
    <line x1="8.6" y1="10.3" x2="11.3" y2="10.3" />
  </svg>
</template>
