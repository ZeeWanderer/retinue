//! Test-only policy experiments. Native completion and cold preparation belong to
//! the measured trace; diagnostics and pixel oracles do not.

mod benchmark;
mod runner;
mod scenarios;
mod tests;

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Cache {
    None,
    Automatic,
    Preferred,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Repair {
    Full,
    Damage,
    Relocate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
struct Policy {
    compiled: bool,
    cache: Cache,
    repair: Repair,
}

impl Policy {
    const DEFAULT: Self = Self {
        compiled: true,
        cache: Cache::Preferred,
        repair: Repair::Relocate,
    };
    const REFERENCE: Self = Self {
        compiled: false,
        cache: Cache::None,
        repair: Repair::Full,
    };

    fn candidates() -> Vec<Self> {
        [
            (false, Cache::None),
            (true, Cache::None),
            (true, Cache::Automatic),
            (true, Cache::Preferred),
        ]
        .into_iter()
        .flat_map(|(compiled, cache)| {
            [Repair::Full, Repair::Damage, Repair::Relocate].map(|repair| Self {
                compiled,
                cache,
                repair,
            })
        })
        .collect()
    }

    fn name(self) -> String {
        let commands = if self.compiled { "compiled" } else { "source" };
        let cache = match self.cache {
            Cache::None => "no-cache",
            Cache::Automatic => "automatic",
            Cache::Preferred => "preferred",
        };
        let repair = match self.repair {
            Repair::Full => "full",
            Repair::Damage => "damage",
            Repair::Relocate => "relocate",
        };
        format!("{commands}/{cache}/{repair}")
    }
}

fn minimize<T: Clone>(input: &[T], mut fails: impl FnMut(&[T]) -> bool) -> Vec<T> {
    let mut result = input.to_vec();
    let mut chunk = result.len().div_ceil(2);
    while chunk > 0 {
        let mut start = 0;
        while start < result.len() {
            let mut trial = result.clone();
            trial.drain(start..(start + chunk).min(trial.len()));
            if !trial.is_empty() && fails(&trial) {
                result = trial;
            } else {
                start += chunk;
            }
        }
        chunk /= 2;
    }
    result
}
