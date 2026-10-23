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
 * 本地凭证加密工具（AES-GCM；密钥由固定口令经 PBKDF2 派生）
 * 作者：JC
 * 用于「凭证加密」开关开启时对主机/会话数据落盘前加密。
 */
const PASSPHRASE = 'jc-shell-local-credential-key'
const SALT = new TextEncoder().encode('jc-shell-salt-v1')
export const ENC_PREFIX = 'enc1:'

let cachedKey: CryptoKey | null = null

async function getKey(): Promise<CryptoKey> {
  if (cachedKey) return cachedKey
  const base = await crypto.subtle.importKey(
    'raw', new TextEncoder().encode(PASSPHRASE), 'PBKDF2', false, ['deriveKey']
  )
  cachedKey = await crypto.subtle.deriveKey(
    { name: 'PBKDF2', salt: SALT, iterations: 50000, hash: 'SHA-256' },
    base,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt']
  )
  return cachedKey
}

export function isEncrypted(v: string): boolean {
  return typeof v === 'string' && v.startsWith(ENC_PREFIX)
}

function toB64(buf: ArrayBuffer): string {
  const bytes = new Uint8Array(buf)
  let bin = ''
  for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i])
  return btoa(bin)
}

function fromB64(s: string): Uint8Array<ArrayBuffer> {
  const bin = atob(s)
  const bytes: Uint8Array<ArrayBuffer> = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i)
  return bytes
}

export async function encryptText(plain: string): Promise<string> {
  const iv = crypto.getRandomValues(new Uint8Array(12))
  const cipher = await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv },
    await getKey(),
    new TextEncoder().encode(plain)
  )
  return ENC_PREFIX + toB64(iv.buffer) + '.' + toB64(cipher)
}

export async function decryptText(payload: string): Promise<string> {
  const [ivB64, dataB64] = payload.slice(ENC_PREFIX.length).split('.')
  const plain = await crypto.subtle.decrypt(
    { name: 'AES-GCM', iv: fromB64(ivB64) },
    await getKey(),
    fromB64(dataB64)
  )
  return new TextDecoder().decode(plain)
}
