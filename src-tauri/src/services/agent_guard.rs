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

//! AI Agent 危险命令分级引擎（本地二次防护，不依赖 AI 自觉）
//! 作者：JC
//!
//! 级别语义:
//! - blocked  致命: 直接禁止执行, 向用户说明原因
//! - danger   高危: 强制二次确认(输入确认词), 醒目警示后果
//! - caution  注意: 正常确认流程, 黄色提示
//! - safe     安全: 常规确认即可
//!
//! 评估取所有命中规则中的最严级别; 与服务端 risk_level 亦取更严者。

use serde::{Deserialize, Serialize};

/// 危险级别（数值越大越严格，便于取 max）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuardLevel {
    Safe = 0,
    Caution = 1,
    Danger = 2,
    Blocked = 3,
}

/// 单条命令的评估结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardVerdict {
    pub level: GuardLevel,
    /// 命中原因（中文，向用户展示）
    pub reason: Option<String>,
}

struct Rule {
    pattern: &'static str,
    level: GuardLevel,
    reason: &'static str,
}

/// 规则表（大小写不敏感；按致命→高危→注意排列，命中取最严）
const RULES: &[Rule] = &[
    // ---------------- blocked 致命 ----------------
    Rule { pattern: r"rm\s+-[a-z]*[rf]+[a-z]*\s+(--no-preserve-root\s+)?/(\s|\*|$)", level: GuardLevel::Blocked, reason: "递归强制删除根目录，将摧毁整个文件系统" },
    Rule { pattern: r"rm\s+-[a-z]*[rf]+[a-z]*\s+~(/|\s|$)", level: GuardLevel::Blocked, reason: "递归强制删除用户主目录，将摧毁全部个人数据" },
    Rule { pattern: r"\bmkfs(\.\w+)?\s", level: GuardLevel::Blocked, reason: "格式化文件系统，磁盘数据将全部丢失" },
    Rule { pattern: r"\bdd\b[^|]*\bof=/dev/(sd|nvme|hd|disk)", level: GuardLevel::Blocked, reason: "直接覆写磁盘设备，数据将全部丢失" },
    Rule { pattern: r":\(\)\s*\{\s*:\|:\&\s*\}\s*;:", level: GuardLevel::Blocked, reason: "fork 炸弹，将耗尽系统进程资源导致死机" },
    Rule { pattern: r"\bformat\s+[a-z]:", level: GuardLevel::Blocked, reason: "格式化磁盘分区，数据将全部丢失" },
    Rule { pattern: r"\b(del|erase)\s+/[a-z]*[sq][a-z]*\s+([a-z]:\\|c:\\windows)", level: GuardLevel::Blocked, reason: "递归删除系统盘/系统目录，系统将不可恢复" },
    Rule { pattern: r"\brd\s+/s\s*/q\s+([a-z]:\\|c:\\windows)", level: GuardLevel::Blocked, reason: "递归删除系统盘/系统目录，系统将不可恢复" },
    Rule { pattern: r#"remove-item\s+(-[a-z]+\s+)*['"]?(c:\\|c:\\windows|[a-z]:\\)['"]?\s*(-[a-z]+\s+)*-recurse"#, level: GuardLevel::Blocked, reason: "递归删除系统盘/系统目录，系统将不可恢复" },
    // ---------------- danger 高危 ----------------
    Rule { pattern: r"\brm\s+-[a-z]*[rf]+[a-z]*\s+", level: GuardLevel::Danger, reason: "递归/强制删除文件，删除后不可恢复" },
    Rule { pattern: r"remove-item\s+.*-recurse", level: GuardLevel::Danger, reason: "递归删除目录及其全部内容，删除后不可恢复" },
    Rule { pattern: r"\b(del|erase)\s+/[a-z]*s[a-z]*", level: GuardLevel::Danger, reason: "递归删除目录内文件，删除后不可恢复" },
    Rule { pattern: r"\brd\s+/s\b", level: GuardLevel::Danger, reason: "递归删除目录及其全部内容，删除后不可恢复" },
    Rule { pattern: r"\b(shutdown|reboot|halt|poweroff|init\s+[06])\b", level: GuardLevel::Danger, reason: "将关机/重启系统，未保存的工作会丢失" },
    Rule { pattern: r"\b(stop-computer|restart-computer)\b", level: GuardLevel::Danger, reason: "将关机/重启系统，未保存的工作会丢失" },
    Rule { pattern: r"\bkill\s+-9\b", level: GuardLevel::Danger, reason: "强制终止进程，目标进程数据可能丢失" },
    Rule { pattern: r"\btaskkill\s+.*(/f|/t)\b", level: GuardLevel::Danger, reason: "强制终止进程，目标进程数据可能丢失" },
    Rule { pattern: r"stop-process\s+.*-force", level: GuardLevel::Danger, reason: "强制终止进程，目标进程数据可能丢失" },
    Rule { pattern: r"\bchmod\s+-r\s+777\s+/", level: GuardLevel::Danger, reason: "递归开放系统目录全部权限，严重安全隐患" },
    Rule { pattern: r"\bchown\s+-r\b", level: GuardLevel::Danger, reason: "递归修改文件属主，可能导致系统/应用无法访问文件" },
    Rule { pattern: r"\b(curl|wget|irm|iwr|invoke-webrequest|invoke-restmethod)\b[^|;]*(\||&&|;)\s*(sudo\s+)?(sh|bash|zsh|powershell|pwsh|iex)\b", level: GuardLevel::Danger, reason: "下载并直接执行远程脚本，存在供应链投毒风险" },
    Rule { pattern: r"\biex\s*\(", level: GuardLevel::Danger, reason: "动态执行字符串代码，来源不明时风险极高" },
    Rule { pattern: r"\bgit\s+clean\s+-[a-z]*[fd]", level: GuardLevel::Danger, reason: "强制清理未跟踪文件，未提交的更改将丢失" },
    Rule { pattern: r"\bgit\s+reset\s+--hard\b", level: GuardLevel::Danger, reason: "硬重置仓库，未提交的工作区更改将丢失" },
    Rule { pattern: r"\bgit\s+push\s+.*(-f|--force)\b", level: GuardLevel::Danger, reason: "强制推送将覆盖远端提交历史" },
    Rule { pattern: r"\breg\s+(delete|add)\b", level: GuardLevel::Danger, reason: "修改注册表，误操作可能导致系统/软件异常" },
    Rule { pattern: r"\b(diskpart|fdisk|parted)\b", level: GuardLevel::Danger, reason: "磁盘分区操作，误操作将导致分区数据丢失" },
    Rule { pattern: r"\b(mkfs|wipefs)\b", level: GuardLevel::Danger, reason: "擦除文件系统签名，数据将丢失" },
    // ---------------- caution 注意 ----------------
    Rule { pattern: r"\b(winget|scoop|choco)\s+(install|uninstall|upgrade)\b", level: GuardLevel::Caution, reason: "安装/卸载软件将修改系统环境" },
    Rule { pattern: r"\b(apt|apt-get|dnf|yum|pacman|brew)\s+(install|remove|purge|upgrade)\b", level: GuardLevel::Caution, reason: "安装/卸载软件包将修改系统环境" },
    Rule { pattern: r"\b(pip|pip3|npm|pnpm|yarn|cargo|gem)\s+(install|uninstall|add|remove)\b", level: GuardLevel::Caution, reason: "安装/卸载依赖包将修改开发环境" },
    Rule { pattern: r"\bsetx\b|setenvironmentvariable", level: GuardLevel::Caution, reason: "永久修改环境变量，影响后续所有会话" },
    Rule { pattern: r"\bsudo\b", level: GuardLevel::Caution, reason: "以管理员/root 权限执行，影响范围扩大" },
    Rule { pattern: r"\b(systemctl|service)\s+\S+\s*(stop|restart|disable|mask)\b", level: GuardLevel::Caution, reason: "停止/禁用系统服务，可能影响依赖它的功能" },
    Rule { pattern: r"\bnet\s+(start|stop)\b|restart-service|stop-service", level: GuardLevel::Caution, reason: "启动/停止系统服务，可能影响依赖它的功能" },
    Rule { pattern: r"\b(mv|move|move-item|rename|rename-item)\s", level: GuardLevel::Caution, reason: "移动/重命名文件，路径引用可能失效" },
    Rule { pattern: r"\b(crontab\s+-[er]|schtasks\s+/create)\b", level: GuardLevel::Caution, reason: "修改定时任务，将在后台长期生效" },
];

/// 评估单条命令（大小写不敏感）
pub fn check_command(cmd: &str) -> GuardVerdict {
    let lower = cmd.to_lowercase();
    let mut best = GuardVerdict { level: GuardLevel::Safe, reason: None };
    for rule in RULES {
        if rule.level <= best.level {
            continue;
        }
        // 规则均为轻量子串/类正则模式; 为免引入 regex 依赖开销, 先用 contains 粗筛再精判
        if pattern_matches(&lower, rule.pattern) {
            best = GuardVerdict { level: rule.level, reason: Some(rule.reason.to_string()) };
        }
    }
    best
}

/// 批量评估，返回最严级别及命中的原因列表
pub fn check_commands(cmds: &[String]) -> GuardVerdict {
    let mut best = GuardVerdict { level: GuardLevel::Safe, reason: None };
    for c in cmds {
        let v = check_command(c);
        if v.level > best.level {
            best = v;
        }
    }
    best
}

// 说明: 规则表中的 pattern 使用 regex 语法以保证表达力。
// 编译期一次性构建 Regex 集合（OnceLock），运行期零开销复用。
fn pattern_matches(text: &str, pattern: &'static str) -> bool {
    use std::sync::OnceLock;
    use std::collections::HashMap;
    static CACHE: OnceLock<std::sync::Mutex<HashMap<&'static str, regex::Regex>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let mut map = cache.lock().unwrap();
    let re = map.entry(pattern).or_insert_with(|| {
        regex::RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .expect("agent_guard 规则编译失败")
    });
    re.is_match(text)
}
