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
 * 窗口拖拽期间的全局"冻结"信号。
 *
 * 背景：窗口移动由主线程消息队列驱动 —— 拖拽时主线程上任何 JS 计算、
 * 样式重算或重排都会直接阻塞窗口移动，表现为拖拽掉帧、一顿一顿。
 *
 * 自定义标题栏按下时 main.ts 会给 body 加 `window-dragging`，冻结指针事件与
 * CSS 过渡/动画（见 style.css），但 setInterval 这类周期性后台任务照跑：
 * 目录自动刷新、SSH cwd 联动同步等每次触发都要发 IPC/SSH 往返并整体重渲染，
 * 拖一次窗口必然撞上几次，是拖拽卡顿的主因。
 *
 * 这里提供一个零开销的同步标志，供这些非关键任务在拖拽期间跳过；
 * 解冻（拖拽结束）时回调订阅者立即补做一次，避免数据长时间停留在旧值。
 */

let frozen = false

/** 解冻回调：拖拽结束后立即补做被跳过的任务 */
const thawHandlers = new Set<() => void>()

/**
 * 热路径判断：拖拽中应跳过非关键后台任务。
 * 刻意不做成响应式 —— 每帧/每次轮询读取不能带来依赖收集开销。
 */
export function isUiFrozen(): boolean {
  return frozen
}

/** 由 main.ts 在窗口拖拽开始 / 结束时调用 */
export function setUiFrozen(next: boolean): void {
  if (frozen === next) return
  frozen = next
  if (frozen) return
  // 解冻：立即补做一次被跳过的任务（复制一份再遍历，允许回调里取消注册）
  for (const fn of Array.from(thawHandlers)) {
    try {
      fn()
    } catch {
      /* 单个订阅者异常不影响其他订阅者 */
    }
  }
}

/** 注册"解冻后补做"回调，返回取消注册函数 */
export function onUiThaw(fn: () => void): () => void {
  thawHandlers.add(fn)
  return () => {
    thawHandlers.delete(fn)
  }
}
