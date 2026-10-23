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

/**
 * 本地文件选择（WebView 原生 input，桌面 + 移动通用）
 * 作者：JC
 *
 * 背景：选文件原本走 Rust 的 rfd 原生对话框，但 rfd 只在桌面可用 ——
 * `pick_key_file` / `pick_import_file` 等在 `#[cfg(mobile)]` 分支直接返回 None，
 * 于是手机上点「浏览密钥」「选择文件」没有任何反应。
 *
 * 方案：改用隐藏的 `<input type="file">`——它是 WebView 的标准能力，
 * 桌面（WebView2/WKWebView）与 Android WebView 都会拉起系统文件选择器，
 * 并且能直接拿到文件内容（File 对象），无需「路径 → 再读盘」的跨进程权限，
 * 天然绕开移动端 SAF/内容 URI 的麻烦。
 */
export interface PickedLocalFile {
  /** 文件名（含扩展名） */
  name: string
  /** 文本内容（按 UTF-8 读取） */
  text: string
}

/**
 * 弹出文件选择器，返回所选文件的「名称 + 文本内容」；用户取消返回空数组。
 * @param opts.accept  同 input.accept，如 '.xsh,.json,.ini' 或 'image/*'
 * @param opts.multiple 是否允许多选
 */
export function pickLocalFileText(
  opts: { accept?: string; multiple?: boolean } = {},
): Promise<PickedLocalFile[]> {
  return new Promise((resolve) => {
    const input = document.createElement('input')
    input.type = 'file'
    if (opts.accept) input.accept = opts.accept
    if (opts.multiple) input.multiple = true
    // 不占布局、不闪烁：挂到 body 但完全隐藏
    input.style.position = 'fixed'
    input.style.left = '-9999px'
    input.style.opacity = '0'

    let settled = false
    const finish = (files: PickedLocalFile[]) => {
      if (settled) return
      settled = true
      input.remove()
      resolve(files)
    }

    const readAll = async () => {
      const list = Array.from(input.files ?? [])
      const out: PickedLocalFile[] = []
      for (const f of list) {
        try {
          out.push({ name: f.name, text: await f.text() })
        } catch {
          // 单个文件读取失败（如二进制/权限）：跳过，不阻断其它文件
        }
      }
      finish(out)
    }

    input.addEventListener('change', () => { void readAll() })
    // 取消：现代内核会派发 cancel；旧内核用「窗口重新获得焦点 + 无文件」兜底
    input.addEventListener('cancel', () => finish([]))
    window.addEventListener('focus', () => {
      window.setTimeout(() => {
        if (!input.files || input.files.length === 0) finish([])
      }, 500)
    }, { once: true })

    document.body.appendChild(input)
    input.click()
  })
}
