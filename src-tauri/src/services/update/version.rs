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

//! 版本号比较工具
//! 作者：JC

pub fn compare_versions(v1: &str, v2: &str) -> i32 {
    let parts1: Vec<&str> = v1.split('.').collect();
    let parts2: Vec<&str> = v2.split('.').collect();
    let max_len = parts1.len().max(parts2.len());
    for i in 0..max_len {
        let p1: u32 = parts1.get(i).and_then(|s| s.parse().ok()).unwrap_or(0);
        let p2: u32 = parts2.get(i).and_then(|s| s.parse().ok()).unwrap_or(0);
        if p1 > p2 { return 1; }
        if p1 < p2 { return -1; }
    }
    0
}