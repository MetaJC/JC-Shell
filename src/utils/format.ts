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
 * 统一字节格式化工具
 * @author JC
 * 规则：< 1KB → B；< 1MB → KB；< 1GB → MB；< 1TB → GB；< 1PB → TB；>= 1PB → PB
 */

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB', 'PB']
const STEP = 1024

/** 完整格式化（含单位、保留小数） */
export function formatSize(bytes: number, decimals = 1): string {
  if (!isFinite(bytes) || bytes <= 0) return '0 B'
  if (bytes < 1) return bytes.toFixed(0) + ' B'
  const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(STEP)), UNITS.length - 1)
  const val = bytes / Math.pow(STEP, exp)
  const fixed = exp === 0 ? val.toFixed(0) : val.toFixed(decimals)
  return `${fixed} ${UNITS[exp]}`
}

/** 紧凑格式化（用于树状视图等空间有限处，保留 0 位小数） */
export function formatSizeShort(bytes: number): string {
  if (!isFinite(bytes) || bytes <= 0) return '-'
  if (bytes < 1024) return bytes + 'B'
  const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(STEP)), UNITS.length - 1)
  const val = bytes / Math.pow(STEP, exp)
  return val.toFixed(0) + UNITS[exp]
}

/** 仅取数值（用于放大显示） */
export function formatSizeValue(bytes: number): { value: string; unit: string } {
  if (!isFinite(bytes) || bytes <= 0) return { value: '0', unit: 'B' }
  if (bytes < 1) return { value: bytes.toFixed(0), unit: 'B' }
  const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(STEP)), UNITS.length - 1)
  const val = bytes / Math.pow(STEP, exp)
  return { value: exp === 0 ? val.toFixed(0) : val.toFixed(2), unit: UNITS[exp] }
}

/** 紧凑无空格（传输队列速度/容量用，如 "12.5MB/s"、"5.2GB"） */
export function formatSizeCompact(bytes: number, decimals = 1): string {
  if (!isFinite(bytes) || bytes <= 0) return '0B'
  if (bytes < 1) return bytes.toFixed(0) + 'B'
  const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(STEP)), UNITS.length - 1)
  const val = bytes / Math.pow(STEP, exp)
  const fixed = exp === 0 ? val.toFixed(0) : val.toFixed(decimals)
  return `${fixed}${UNITS[exp]}`
}
