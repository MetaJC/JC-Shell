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

//! 本机指标后台采样（CPU / 内存 / 交换分区 / 磁盘 / 网络速率 / 静态系统信息）
//! 作者：JC
//!
//! 为什么要有这个模块：
//! CPU 使用率与网络速率都必须「两次取样求差」，早期实现直接在 `#[tauri::command]`
//! 的**同步**函数里 `std::thread::sleep(300ms / 500ms)` 凑采样窗口。
//! 而 Tauri 的同步命令是在**主线程（UI 线程）**上执行的：同一个进程里
//! main / term-* / settings / tab-ghost 所有窗口共用这一条线程，
//! 于是前端每轮轮询（终端系统状态条 2s、设备信息面板 5s）都会把整个应用冻住
//! 0.8~1 秒；打开「设备信息」面板或拖出一个独立窗口（新窗口挂载即开始轮询）时，
//! 表现就是「主窗口和新窗口一起卡死」。
//!
//! 现在改为：一条常驻后台线程按秒采样并写入内存快照，命令只做一次读锁拷贝
//! （微秒级、零 IO、零睡眠），UI 线程永远不会因为采样而阻塞。
#![cfg(desktop)]

use std::sync::RwLock;
use std::time::Duration;

use once_cell::sync::Lazy;
use sysinfo::{Disks, Networks, System};

/// 采样周期（毫秒）：1s 足够 UI 展示，也让 CPU/网络的差分窗口稳定
const SAMPLE_INTERVAL_MS: u64 = 1000;
/// 磁盘列表采样周期（每 N 轮采一次）：Windows 上枚举卷/查询可用空间较慢，
/// 且离线网络盘可能阻塞，必须远离高频路径
const DISK_EVERY_TICKS: u32 = 10;
/// 本机出口 IP 采样周期（每 N 轮采一次）
const IP_EVERY_TICKS: u32 = 5;

/// 一块磁盘分区
#[derive(Clone, Default)]
pub struct DiskPartitionSnapshot {
    pub mount: String,
    pub total: u64,
    pub used: u64,
}

/// 静态系统信息（主机名/系统版本/CPU 型号等，运行期基本不变）
#[derive(Clone, Default)]
pub struct SystemInfoSnapshot {
    pub hostname: String,
    pub os: String,
    pub kernel: String,
    pub uptime: String,
    pub cpu_model: String,
    pub cpu_cores: usize,
    pub ip: String,
}

/// 一次采样得到的全部指标
#[derive(Clone, Default)]
pub struct MetricsSnapshot {
    pub cpu_usage: f64,
    pub per_core: Vec<f64>,
    pub load_avg: [f64; 3],
    pub mem_total: u64,
    pub mem_used: u64,
    pub mem_available: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub net_rx_rate: u64,
    pub net_tx_rate: u64,
    pub disks: Vec<DiskPartitionSnapshot>,
    pub disk_total: u64,
    pub disk_used: u64,
    pub sys: SystemInfoSnapshot,
    /// 是否已完成首轮采样（首轮前命令返回零值，避免 UI 线程等待）
    pub ready: bool,
}

static SNAPSHOT: Lazy<RwLock<MetricsSnapshot>> = Lazy::new(|| RwLock::new(MetricsSnapshot::default()));

/// 采样线程启动器：首次取快照时启动，仅启动一次
static SAMPLER: Lazy<()> = Lazy::new(|| {
    let spawned = std::thread::Builder::new()
        .name("jc-local-metrics".into())
        .spawn(sample_loop);
    if let Err(e) = spawned {
        // 采样线程起不来也不能让命令失败：命令读到默认快照即可
        tracing::warn!("本机指标采样线程启动失败: {e}");
    }
});

/// 读取最近一次采样快照（微秒级，绝不阻塞 UI 线程）
pub fn snapshot() -> MetricsSnapshot {
    Lazy::force(&SAMPLER);
    SNAPSHOT
        .read()
        .map(|s| s.clone())
        .unwrap_or_default()
}

fn sample_loop() {
    let mut sys = System::new();
    // 首轮建立基线（CPU/网络差值都需要两次取样）
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    let mut nets = Networks::new_with_refreshed_list();
    let mut last_rx = total_rx(&nets);
    let mut last_tx = total_tx(&nets);
    let mut static_info = collect_static_info(&sys);
    // 立即发布一次初始快照（静态系统信息 / 磁盘立刻可用），
    // 否则首轮采样前的 1 秒内命令只能返回空值，界面会短暂显示为空白
    let mut disks: Vec<DiskPartitionSnapshot> = collect_disks();
    let disk_total = disks.iter().map(|d| d.total).sum();
    let disk_used = disks.iter().map(|d| d.used).sum();
    if let Ok(mut w) = SNAPSHOT.write() {
        *w = MetricsSnapshot {
            disks: disks.clone(),
            disk_total,
            disk_used,
            sys: static_info.clone(),
            ..Default::default()
        };
    }
    let mut ticks: u32 = 0;

    loop {
        std::thread::sleep(Duration::from_millis(SAMPLE_INTERVAL_MS));
        ticks = ticks.wrapping_add(1);

        sys.refresh_cpu_usage();
        sys.refresh_memory();
        nets.refresh();

        let rx = total_rx(&nets);
        let tx = total_tx(&nets);
        let secs = SAMPLE_INTERVAL_MS as f64 / 1000.0;
        let net_rx_rate = ((rx.saturating_sub(last_rx)) as f64 / secs).round() as u64;
        let net_tx_rate = ((tx.saturating_sub(last_tx)) as f64 / secs).round() as u64;
        last_rx = rx;
        last_tx = tx;

        let cpu_usage = sys.global_cpu_usage() as f64;
        let per_core: Vec<f64> = sys.cpus().iter().map(|c| c.cpu_usage() as f64).collect();

        // 运行时长会随时间变化，随采样一起刷新；其余字段保持不变
        static_info.uptime = format_uptime(System::uptime());
        if ticks % IP_EVERY_TICKS == 0 {
            static_info.ip = local_ip().unwrap_or_default();
        }
        if ticks % DISK_EVERY_TICKS == 1 {
            disks = collect_disks();
        }
        let disk_total = disks.iter().map(|d| d.total).sum();
        let disk_used = disks.iter().map(|d| d.used).sum();

        let next = MetricsSnapshot {
            cpu_usage: (cpu_usage * 10.0).round() / 10.0,
            per_core,
            load_avg: load_avg(&sys, cpu_usage),
            mem_total: sys.total_memory(),
            mem_used: sys.used_memory(),
            mem_available: sys.available_memory(),
            swap_total: sys.total_swap(),
            swap_used: sys.used_swap(),
            net_rx_rate,
            net_tx_rate,
            disks: disks.clone(),
            disk_total,
            disk_used,
            sys: static_info.clone(),
            ready: true,
        };
        match SNAPSHOT.write() {
            Ok(mut w) => *w = next,
            Err(_) => return, // 锁中毒（理论上不会发生）：停掉采样线程，命令继续读旧快照
        }
    }
}

fn total_rx(nets: &Networks) -> u64 {
    nets.iter().map(|(_, d)| d.received()).sum()
}

fn total_tx(nets: &Networks) -> u64 {
    nets.iter().map(|(_, d)| d.transmitted()).sum()
}

/// 静态系统信息（CPU 型号需要先 refresh 过 cpus 列表）
fn collect_static_info(sys: &System) -> SystemInfoSnapshot {
    let os = match (System::name(), System::os_version()) {
        (Some(name), Some(ver)) => format!("{name} {ver}"),
        (Some(name), None) => name,
        _ => String::new(),
    };
    SystemInfoSnapshot {
        hostname: System::host_name().unwrap_or_default(),
        os,
        kernel: System::kernel_version().unwrap_or_default(),
        uptime: format_uptime(System::uptime()),
        cpu_model: sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .unwrap_or_default(),
        cpu_cores: sys.physical_core_count().unwrap_or_else(|| sys.cpus().len()),
        ip: local_ip().unwrap_or_default(),
    }
}

/// 磁盘分区（复用 sysinfo，跨平台）
fn collect_disks() -> Vec<DiskPartitionSnapshot> {
    let disks = Disks::new_with_refreshed_list();
    let mut list = Vec::new();
    for d in disks.iter() {
        let total = d.total_space();
        if total == 0 {
            continue;
        }
        list.push(DiskPartitionSnapshot {
            mount: d.mount_point().to_string_lossy().to_string(),
            total,
            used: total.saturating_sub(d.available_space()),
        });
    }
    list
}

/// 运行时长格式化
fn format_uptime(secs: u64) -> String {
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("运行 {d} 天 {h} 小时 {m} 分钟")
    } else if h > 0 {
        format!("运行 {h} 小时 {m} 分钟")
    } else {
        format!("运行 {m} 分钟")
    }
}

/// 本机出口 IP（UDP 连接探测，不真正发包，不会阻塞）
fn local_ip() -> Option<String> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    Some(sock.local_addr().ok()?.ip().to_string())
}

/// 负载均值：Linux/macOS 有原生值；Windows 用 使用率×核心数 近似
fn load_avg(sys: &System, usage: f64) -> [f64; 3] {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let avg = sysinfo::System::load_average();
        [avg.one, avg.five, avg.fifteen]
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let cores = sys.cpus().len().max(1) as f64;
        let approx = (usage / 100.0 * cores * 100.0).round() / 100.0;
        [approx, approx, approx]
    }
}
