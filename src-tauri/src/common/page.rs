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

//! 分页结果（全系统统一结构，所有列表接口返回此结构）
//! 作者：JC

use serde::Serialize;

use super::constants::{DEFAULT_PAGE_NUM, DEFAULT_PAGE_SIZE};

/// 通用分页响应
#[derive(Debug, Clone, Serialize)]
pub struct Page<T> {
    pub list: Vec<T>,
    pub total: u64,
    pub page_num: u32,
    pub page_size: u32,
}

impl<T> Default for Page<T> {
    fn default() -> Self {
        Self {
            list: vec![],
            total: 0,
            page_num: DEFAULT_PAGE_NUM,
            page_size: DEFAULT_PAGE_SIZE,
        }
    }
}

impl<T> Page<T> {
    pub fn new(list: Vec<T>, total: u64, page_num: u32, page_size: u32) -> Self {
        Self {
            list,
            total,
            page_num: if page_num == 0 { DEFAULT_PAGE_NUM } else { page_num },
            page_size: if page_size == 0 { DEFAULT_PAGE_SIZE } else { page_size },
        }
    }
}

/// 分页请求参数（统一入参）
#[derive(Debug, Clone, Copy)]
pub struct PageQuery {
    pub page_num: u32,
    pub page_size: u32,
    pub offset: u64,
}

impl PageQuery {
    pub fn from(page_num: Option<u32>, page_size: Option<u32>) -> Self {
        let page_num = page_num.unwrap_or(DEFAULT_PAGE_NUM).max(1);
        let page_size = page_size.unwrap_or(DEFAULT_PAGE_SIZE).min(500).max(1);
        let offset = (page_num as u64 - 1) * page_size as u64;
        Self { page_num, page_size, offset }
    }
}
