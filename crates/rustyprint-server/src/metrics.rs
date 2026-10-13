//! Prometheus metrics, kept in memory and written in the text exposition format.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

/// Upper bounds of the render time histogram's buckets, in seconds.
const BUCKETS: [f64; 10] = [0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0];

#[derive(Default)]
pub(crate) struct Metrics {
    requests: Mutex<BTreeMap<u16, u64>>,
    errors: Mutex<BTreeMap<&'static str, u64>>,
    renders: Mutex<Histogram>,
    in_flight: AtomicI64,
}

#[derive(Default)]
struct Histogram {
    buckets: [u64; BUCKETS.len()],
    count: u64,
    sum: f64,
}

/// Counts a render as running until it is dropped.
pub(crate) struct InFlight<'a>(&'a AtomicI64);

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Metrics {
    pub(crate) fn request(&self, status: u16) {
        *lock(&self.requests).entry(status).or_default() += 1;
    }

    pub(crate) fn error(&self, kind: &'static str) {
        *lock(&self.errors).entry(kind).or_default() += 1;
    }

    pub(crate) fn render_started(&self) -> InFlight<'_> {
        self.in_flight.fetch_add(1, Ordering::Relaxed);
        InFlight(&self.in_flight)
    }

    pub(crate) fn render_finished(&self, elapsed: Duration) {
        let seconds = elapsed.as_secs_f64();
        let mut renders = lock(&self.renders);
        for (bucket, bound) in renders.buckets.iter_mut().zip(BUCKETS) {
            if seconds <= bound {
                *bucket += 1;
            }
        }
        renders.count += 1;
        renders.sum += seconds;
    }

    /// The metrics in the Prometheus text format.
    pub(crate) fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("# HELP rustyprint_requests_total HTTP requests answered, by status.\n");
        out.push_str("# TYPE rustyprint_requests_total counter\n");
        for (status, count) in lock(&self.requests).iter() {
            let _ = writeln!(
                out,
                "rustyprint_requests_total{{status=\"{status}\"}} {count}"
            );
        }
        out.push_str("# HELP rustyprint_errors_total Failed render requests, by kind.\n");
        out.push_str("# TYPE rustyprint_errors_total counter\n");
        for (kind, count) in lock(&self.errors).iter() {
            let _ = writeln!(out, "rustyprint_errors_total{{kind=\"{kind}\"}} {count}");
        }
        out.push_str("# HELP rustyprint_render_seconds Time spent rendering.\n");
        out.push_str("# TYPE rustyprint_render_seconds histogram\n");
        let renders = lock(&self.renders);
        for (bound, count) in BUCKETS.iter().zip(renders.buckets) {
            let _ = writeln!(
                out,
                "rustyprint_render_seconds_bucket{{le=\"{bound}\"}} {count}"
            );
        }
        let _ = writeln!(
            out,
            "rustyprint_render_seconds_bucket{{le=\"+Inf\"}} {}",
            renders.count
        );
        let _ = writeln!(out, "rustyprint_render_seconds_sum {}", renders.sum);
        let _ = writeln!(out, "rustyprint_render_seconds_count {}", renders.count);
        out.push_str("# HELP rustyprint_renders_in_flight Renders running now.\n");
        out.push_str("# TYPE rustyprint_renders_in_flight gauge\n");
        let _ = writeln!(
            out,
            "rustyprint_renders_in_flight {}",
            self.in_flight.load(Ordering::Relaxed)
        );
        out
    }
}

/// A lock that a panic while it was held does not poison for the metrics.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}
