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

//! 公共模块
//! 作者：JC

pub mod error;
pub mod result;
pub mod constants;
pub mod security;
pub mod page;
pub mod paths;
pub mod http;
pub mod blocking;

pub use error::{AppError, AppErrorPayload};
pub use result::AppResult;
pub use constants::*;
pub use page::{Page, PageQuery};
