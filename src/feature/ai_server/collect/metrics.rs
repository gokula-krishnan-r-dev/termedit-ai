use anyhow::Result;
use std::time::Duration;
use sysinfo::{Disks, System};

use crate::feature::ai_server::context::{DiskMetric, MetricsSnapshot, ProcessMetric, ServerContext};

use super::CollectConfig;

pub async fn contribute(ctx: &mut ServerContext, config: &CollectConfig) -> Result<()> {
    let top_n = config.top_processes;
    let snap = tokio::task::spawn_blocking(move || collect_sync(top_n)).await??;
    ctx.metrics = Some(snap);
    Ok(())
}

fn collect_sync(top_n: usize) -> Result<MetricsSnapshot> {
    let mut sys = System::new_all();
    std::thread::sleep(Duration::from_millis(200));
    sys.refresh_cpu_usage();

    let cpus = sys.cpus().len();
    let total_memory_bytes = sys.total_memory();
    let used_memory_bytes = sys.used_memory();

    let load_average = System::load_average();

    let disks_list = Disks::new_with_refreshed_list();
    let mut disks = vec![];
    for d in disks_list.list() {
        disks.push(DiskMetric {
            mount: d.mount_point().to_string_lossy().to_string(),
            total_bytes: d.total_space(),
            available_bytes: d.available_space(),
        });
    }

    let mut procs: Vec<ProcessMetric> = sys
        .processes()
        .iter()
        .map(|(pid, p)| ProcessMetric {
            pid: pid.as_u32(),
            name: p.name().to_string_lossy().into_owned(),
            memory_bytes: p.memory(),
            cpu_usage: p.cpu_usage(),
        })
        .collect();
    procs.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes));
    procs.truncate(top_n.max(1));

    Ok(MetricsSnapshot {
        cpus,
        total_memory_bytes,
        used_memory_bytes,
        load_average: Some((load_average.one, load_average.five, load_average.fifteen)),
        disks,
        top_processes: procs,
    })
}
