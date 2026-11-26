use retinue_ui::scene::census::solver::{COLUMNS, Rows};
use serde_json::{Value, json};

pub(super) struct Trace {
    pub(super) timed: Option<bool>,
    pub(super) last: Rows,
    samples: Vec<Value>,
}

impl Trace {
    pub(super) fn configured() -> Self {
        Self {
            timed: std::env::var("GUI_BENCH_SOLVER")
                .ok()
                .map(|v| match v.as_str() {
                    "counts" => false,
                    "timed" => true,
                    _ => panic!("expected counts or timed"),
                }),
            last: Rows::new(),
            samples: Vec::new(),
        }
    }
    pub(super) fn record(&mut self, label: &str) {
        if self.timed.is_some() {
            self.samples
                .push(json!([label, self.last.iter().collect::<Vec<_>>()]));
        }
    }
    pub(super) fn attach(&self, report: &mut Value, skip: usize) {
        if let Some(timed) = self.timed {
            report["solver"] = json!({"timed":timed,"columns":COLUMNS,
                "group":["kind","mode","known_axes","bounded_space","custom_algorithm_on_miss"],
                "samples":&self.samples[skip..]});
        }
    }
}
