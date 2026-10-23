// Copyright 2024-2026 OpenJC (https://www.openjc.cc)
// 
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// 
//     http://www.apache.org/licenses/LICENSE-2.0
// 
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'
import pkg from './package.json'

// 移动端（android dev）调试时 Tauri CLI 会注入 TAURI_DEV_HOST（本机局域网 IP），
// vite 必须监听该地址，模拟器/真机才能访问 dev server；桌面端保持 127.0.0.1。
const host = process.env.TAURI_DEV_HOST

export default defineConfig({
  plugins: [vue()],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
    // 版本标识：本工程固定为 community。
    // 前端据此把软件名显示为「JC Shell」。
    __EDITION__: JSON.stringify('community')
  },
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src')
    }
  },
  server: {
    // 桌面端强制监听 IPv4 127.0.0.1：本机 localhost 只解析到 IPv6(::1) 时，
    // 默认绑定的 ::1 会导致 WebView2 通过 IPv4 连接 localhost 失败；
    // 且 ::1 不在系统代理 bypass(127.* / localhost) 列表内，易被代理软件拦截。
    // devUrl 同步改为 http://127.0.0.1:1420，保证 WebView2 直连（bypass 代理）。
    // 移动端（TAURI_DEV_HOST 存在）则监听该局域网 IP，并配置对应 HMR。
    host: host || '127.0.0.1',
    port: 1420,
    strictPort: true,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: {
      ignored: ['**/src-tauri/**']
    }
  },
  clearScreen: false
})
