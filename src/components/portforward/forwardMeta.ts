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
 * 端口转发的「架构图元数据」
 * 作者：JC
 * 图示节点、配置项归属节点、向导步骤集中定义在这里，
 * 保证「示意图」「配置表单」「交互式向导」三处使用同一份结构，改一处即同步。
 *
 * 节点顺序即消息前进顺序：client → internet → ssh → target（防火墙是被挡住的装饰元素，不是节点）。
 */
import type { ForwardMode, ForwardNodeId, ForwardRule } from '@/stores/portForward'

export interface NodeDef {
  id: ForwardNodeId
  labelKey: string
  /** 紧凑模式（模式选择卡片）用的短标签 */
  shortKey: string
}

/**
 * 聚焦某个表单字段时要高亮的图中元素。
 * 比 node 更细一档：目标主机端口对应「目标服务」这个独立元素，
 * 目标主机地址对应目标主机设备本身。
 */
export type ForwardHotKey = ForwardNodeId | 'targetService'

export interface FieldDef {
  node: ForwardNodeId
  key: keyof ForwardRule
  labelKey: string
  type: 'text' | 'port' | 'auth' | 'password' | 'key'
  /** 聚焦该字段时高亮的元素；缺省按 node 处理 */
  hot?: ForwardHotKey
}

/** 交互式向导的一步 */
export interface WizardStep {
  titleKey: string
  descKey: string
  /** 该步骤聚焦的节点（图示中高亮） */
  node: ForwardNodeId
  fields: FieldDef[]
}

/** 节点顺序：连接段 i 连接 节点 i → 节点 i+1（共 3 段） */
export const NODE_ORDER: ForwardNodeId[] = ['client', 'internet', 'ssh', 'target']

const SSH_FIELDS: FieldDef[] = [
  { node: 'ssh', key: 'sshHost', labelKey: 'portForward.field.sshHost', type: 'text' },
  { node: 'ssh', key: 'sshPort', labelKey: 'portForward.field.sshPort', type: 'port' },
  { node: 'ssh', key: 'sshUser', labelKey: 'portForward.field.sshUser', type: 'text' },
  { node: 'ssh', key: 'sshAuthType', labelKey: 'portForward.field.sshAuthType', type: 'auth' },
  { node: 'ssh', key: 'sshPassword', labelKey: 'portForward.field.sshPassword', type: 'password' },
  { node: 'ssh', key: 'sshKeyPath', labelKey: 'portForward.field.sshKey', type: 'key' },
]

const TARGET_LOCAL_FIELDS: FieldDef[] = [
  { node: 'target', key: 'remoteTargetHost', labelKey: 'portForward.field.targetHost', type: 'text' },
  // 目标端口 = 最终要访问的那个服务，故高亮右侧的「目标服务」元素
  { node: 'target', key: 'remoteTargetPort', labelKey: 'portForward.field.targetPort', type: 'port', hot: 'targetService' },
]

const TARGET_REMOTE_FIELDS: FieldDef[] = [
  { node: 'target', key: 'localTargetHost', labelKey: 'portForward.field.localTargetHost', type: 'text' },
  { node: 'target', key: 'localTargetPort', labelKey: 'portForward.field.localTargetPort', type: 'port' },
]

export const NODES: Record<ForwardMode, NodeDef[]> = {
  local: [
    { id: 'client', labelKey: 'portForward.node.localClient', shortKey: 'portForward.nodeShort.localClient' },
    { id: 'internet', labelKey: 'portForward.node.tunnel', shortKey: 'portForward.nodeShort.tunnel' },
    { id: 'ssh', labelKey: 'portForward.node.sshServer', shortKey: 'portForward.nodeShort.sshServer' },
    { id: 'target', labelKey: 'portForward.node.targetHost', shortKey: 'portForward.nodeShort.target' },
  ],
  remote: [
    { id: 'client', labelKey: 'portForward.node.remoteClient', shortKey: 'portForward.nodeShort.remoteClient' },
    { id: 'internet', labelKey: 'portForward.node.tunnel', shortKey: 'portForward.nodeShort.tunnel' },
    { id: 'ssh', labelKey: 'portForward.node.sshServer', shortKey: 'portForward.nodeShort.sshServer' },
    { id: 'target', labelKey: 'portForward.node.localService', shortKey: 'portForward.nodeShort.target' },
  ],
  dynamic: [
    { id: 'client', labelKey: 'portForward.node.localClient', shortKey: 'portForward.nodeShort.localClient' },
    { id: 'internet', labelKey: 'portForward.node.tunnel', shortKey: 'portForward.nodeShort.tunnel' },
    { id: 'ssh', labelKey: 'portForward.node.sshServer', shortKey: 'portForward.nodeShort.sshServer' },
    { id: 'target', labelKey: 'portForward.node.targetNetwork', shortKey: 'portForward.nodeShort.target' },
  ],
}

export const FIELDS: Record<ForwardMode, FieldDef[]> = {
  local: [
    { node: 'client', key: 'bindAddr', labelKey: 'portForward.field.bindAddr', type: 'text' },
    { node: 'client', key: 'localPort', labelKey: 'portForward.field.localPort', type: 'port' },
    ...SSH_FIELDS,
    ...TARGET_LOCAL_FIELDS,
  ],
  remote: [
    ...SSH_FIELDS,
    // 远程模式：这是「在 SSH 服务器上监听」的地址（127.0.0.1 = 仅服务器本机可访问，
    // 0.0.0.0 = 服务器所在网络都能访问）
    { node: 'ssh', key: 'bindAddr', labelKey: 'portForward.field.remoteBindAddr', type: 'text' },
    { node: 'ssh', key: 'remotePort', labelKey: 'portForward.field.remotePort', type: 'port' },
    ...TARGET_REMOTE_FIELDS,
  ],
  dynamic: [
    { node: 'client', key: 'bindAddr', labelKey: 'portForward.field.bindAddr', type: 'text' },
    { node: 'client', key: 'localPort', labelKey: 'portForward.field.socksPort', type: 'port' },
    ...SSH_FIELDS,
    // 动态模式的目标网络本身由 SOCKS 请求决定，无固定地址；
    // 下面两个字段仅用于「测试连通性」时可选指定的测试目标，真实探测该目标是否可达
    { node: 'target', key: 'remoteTargetHost', labelKey: 'portForward.field.testTargetHost', type: 'text' },
    { node: 'target', key: 'remoteTargetPort', labelKey: 'portForward.field.testTargetPort', type: 'port' },
  ],
}

export const WIZARD_STEPS: Record<ForwardMode, WizardStep[]> = {
  local: [
    { titleKey: 'portForward.step.listen', descKey: 'portForward.step.listenDesc', node: 'client', fields: [{ node: 'client', key: 'localPort', labelKey: 'portForward.field.localPort', type: 'port' }] },
    { titleKey: 'portForward.step.ssh', descKey: 'portForward.step.sshDesc', node: 'ssh', fields: SSH_FIELDS },
    { titleKey: 'portForward.step.target', descKey: 'portForward.step.targetDesc', node: 'target', fields: TARGET_LOCAL_FIELDS },
  ],
  remote: [
    { titleKey: 'portForward.step.sshRemote', descKey: 'portForward.step.sshRemoteDesc', node: 'ssh', fields: [...SSH_FIELDS, { node: 'ssh', key: 'remotePort', labelKey: 'portForward.field.remotePort', type: 'port' }] },
    { titleKey: 'portForward.step.localTarget', descKey: 'portForward.step.localTargetDesc', node: 'target', fields: TARGET_REMOTE_FIELDS },
  ],
  dynamic: [
    { titleKey: 'portForward.step.socks', descKey: 'portForward.step.socksDesc', node: 'client', fields: [{ node: 'client', key: 'localPort', labelKey: 'portForward.field.socksPort', type: 'port' }] },
    { titleKey: 'portForward.step.ssh', descKey: 'portForward.step.sshDesc', node: 'ssh', fields: SSH_FIELDS },
  ],
}

/** 某节点的配置项 */
export function fieldsOfNode(mode: ForwardMode, node: ForwardNodeId): FieldDef[] {
  return FIELDS[mode].filter(f => f.node === node)
}
