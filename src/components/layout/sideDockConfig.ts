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
 * 侧边 Dock 栏尺寸常量
 * 作者：JC
 * SideDock 组件与需要「避让」它的布局（如左下角头像）共用同一份数值，
 * 避免两处各写一份、改了一处导致错位或遮挡。
 */

/** 固定显示（展开）时 Dock 占据的宽度（px） */
export const SIDE_DOCK_WIDTH = 40
/** 自动隐藏时贴边提示条的宽度（px） */
export const SIDE_DOCK_HINT_WIDTH = 8
