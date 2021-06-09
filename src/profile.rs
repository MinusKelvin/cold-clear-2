use std::collections::HashMap;
use std::io::Write;
use std::thread::ThreadId;
use std::time::Duration;
use std::time::Instant;

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use parking_lot::RwLock;

static DATA: Lazy<RwLock<HashMap<ThreadId, Mutex<ThreadData>>>> = Lazy::new(Default::default);

pub fn setup_thread() {
    DATA.write()
        .insert(std::thread::current().id(), Default::default());
}

#[derive(Default)]
struct ThreadData {
    stack: Vec<ScopeInfo>,
    aggregates: HashMap<&'static str, ProfileData>,
}

struct ScopeInfo {
    name: &'static str,
    start: Instant,
    non_self_time: Duration,
}

#[derive(Default)]
struct ProfileData {
    invocations: u32,
    total_time: Duration,
    self_time: Duration,
}

pub struct ProfileScope {
    _priv: (),
}

impl ProfileScope {
    pub fn new(name: &'static str) -> Self {
        let guard = DATA.read();
        let mut data = guard.get(&std::thread::current().id()).unwrap().lock();
        data.stack.push(ScopeInfo {
            name,
            start: Instant::now(),
            non_self_time: Duration::new(0, 0),
        });
        ProfileScope { _priv: () }
    }
}

impl Drop for ProfileScope {
    fn drop(&mut self) {
        let guard = DATA.read();
        let mut data = guard.get(&std::thread::current().id()).unwrap().lock();
        let frame = data.stack.pop().unwrap();
        let elapsed = frame.start.elapsed();
        if let Some(parent) = data.stack.last_mut() {
            parent.non_self_time += elapsed;
        }
        let data = data.aggregates.entry(frame.name).or_default();
        data.invocations += 1;
        data.self_time += elapsed - frame.non_self_time;
        data.total_time += elapsed;
    }
}

pub fn profiling_frame_end(nodes: u64, time: Duration) {
    let report = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open("profile.txt")
        .unwrap();
    let mut report = std::io::BufWriter::new(report);

    let mut raw_data = DATA.write();
    let mut data = HashMap::new();
    for (name, d) in raw_data
        .values_mut()
        .flat_map(|d| d.get_mut().aggregates.drain())
    {
        let data = data.entry(name).or_insert_with(ProfileData::default);
        data.self_time += d.self_time;
        data.total_time += d.total_time;
        data.invocations += d.invocations;
    }

    let total_time: Duration = data.values().map(|d| d.self_time).sum();
    writeln!(
        report,
        "{} nodes in {:.2?} ({:.1} kn/s)",
        nodes,
        time,
        nodes as f64 / time.as_secs_f64() / 1000.0
    )
    .unwrap();
    writeln!(report, "Total CPU time measured: {:.2?}", total_time).unwrap();

    let mut data: Vec<_> = data.into_iter().collect();
    data.sort_by_key(|(_, d)| std::cmp::Reverse(d.self_time));
    for (name, data) in data {
        writeln!(
            report,
            "{name:20} {spent:.2?} ({percent:.2}%) ({self:.2}% self) i: {invocations} avg: {avg:.2?}",
            name = name,
            spent = data.self_time,
            invocations = data.invocations,
            percent = data.self_time.as_secs_f64() / total_time.as_secs_f64() * 100.0,
            self = data.self_time.as_secs_f64() / data.total_time.as_secs_f64() * 100.0,
            avg = data.total_time / data.invocations
        )
        .unwrap();
    }
    writeln!(report).unwrap();
}
