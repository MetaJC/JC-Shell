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

//! 数据模型聚合
//! 作者：JC

pub mod host;
pub mod terminal;
pub mod ai;
pub mod plugin;
pub mod setting;

pub use host::*;
pub use terminal::*;
pub use ai::*;
pub use plugin::*;
pub use setting::*;
