use rustix::time::{ClockId, clock_gettime};
use std::{fs::File, os::unix::fs::FileExt, time::Instant};

pub(super) const STAGES: [&str; 7] = [
    "input",
    "tick",
    "painter_begin",
    "prepare_scene",
    "prepare_rasters",
    "submit",
    "finish",
];

pub(super) struct Probe {
    schedstat: Option<File>,
}

#[derive(Clone, Copy)]
struct Stamp {
    wall: Instant,
    counters: [u64; 3],
}

fn cpu(id: ClockId) -> u64 {
    let t = clock_gettime(id);
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}

fn queue_ns(file: &File) -> u64 {
    let mut buf = [0; 128];
    let n = file.read_at(&mut buf, 0).expect("read thread schedstat");
    std::str::from_utf8(&buf[..n])
        .unwrap()
        .split_whitespace()
        .nth(1)
        .expect("schedstat run delay")
        .parse()
        .unwrap()
}

impl Probe {
    pub(super) fn configured() -> Option<Self> {
        let mode = std::env::var("GUI_BENCH_SCROLL_PHASES").ok()?;
        Some(Self {
            schedstat: match mode.as_str() {
                "clocks" => None,
                "scheduler" => {
                    Some(File::open("/proc/thread-self/schedstat").expect("open owner schedstat"))
                }
                _ => panic!("GUI_BENCH_SCROLL_PHASES must be clocks or scheduler"),
            },
        })
    }

    fn stamp(&self) -> Stamp {
        Stamp {
            wall: Instant::now(),
            counters: [
                cpu(ClockId::ThreadCPUTime),
                cpu(ClockId::ProcessCPUTime),
                self.schedstat.as_ref().map_or(0, queue_ns),
            ],
        }
    }

    pub(super) fn start(&self) -> Frame<'_> {
        Frame {
            probe: self,
            previous: self.stamp(),
            ns: [[0; 4]; STAGES.len()],
            count: 0,
        }
    }
    pub(super) fn has_scheduler(&self) -> bool {
        self.schedstat.is_some()
    }
}

pub(super) struct Frame<'a> {
    probe: &'a Probe,
    previous: Stamp,
    ns: [[u64; 4]; STAGES.len()],
    count: usize,
}

impl Frame<'_> {
    pub(super) fn mark(&mut self) {
        let now = self.probe.stamp();
        let prev = std::mem::replace(&mut self.previous, now);
        let [owner, process, queue] = std::array::from_fn(|i| now.counters[i] - prev.counters[i]);
        self.ns[self.count] = [
            (now.wall - prev.wall).as_nanos() as u64,
            owner,
            process,
            queue,
        ];
        self.count += 1;
    }

    pub(super) fn finish(self) -> [[u64; 4]; STAGES.len()] {
        assert_eq!(self.count, STAGES.len());
        self.ns
    }
}
