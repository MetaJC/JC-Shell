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

//! Tauri Commands 聚合
//! 作者：JC

pub mod terminal_cmd;
pub mod host_cmd;
pub mod ai_cmd;
pub mod plugin_cmd;
pub mod setting_cmd;
pub mod auth_cmd;
pub mod window_cmd;
pub mod update_cmd;
pub mod qq_group_cmd;
pub mod ai_agent_cmd;
pub mod sync_cmd;
pub mod client_error_log_cmd;
pub mod launch_dialog_cmd;
pub mod ssh_key_cmd;
pub mod security_cmd;
pub mod session_import_cmd;
pub mod port_forward_cmd;
pub mod telemetry_cmd;
pub mod legal_agreement_cmd;

pub use terminal_cmd::*;
pub use host_cmd::*;
pub use ai_cmd::*;
pub use plugin_cmd::*;
pub use setting_cmd::*;
pub use auth_cmd::*;
pub use window_cmd::*;
pub use update_cmd::*;
pub use qq_group_cmd::*;
pub use ai_agent_cmd::*;
pub use sync_cmd::*;
pub use client_error_log_cmd::*;
pub use launch_dialog_cmd::*;
pub use ssh_key_cmd::*;
pub use security_cmd::*;
pub use session_import_cmd::*;
pub use port_forward_cmd::*;
pub use telemetry_cmd::*;
pub use legal_agreement_cmd::*;