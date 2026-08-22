//! Low-cardinality in-process node health and metrics.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const LIVENESS_STALE_AFTER: Duration = Duration::from_secs(5);

struct Inner {
    started: Instant,
    started_at_seconds: u64,
    live: AtomicBool,
    ready: AtomicBool,
    draining: AtomicBool,
    listeners_expected: AtomicU64,
    listeners_ready: AtomicU64,
    last_progress_ms: AtomicU64,
    connections: AtomicU64,
    max_connections: AtomicU64,
    reservations: AtomicU64,
    max_reservations: AtomicU64,
    circuits: AtomicU64,
    max_circuits: AtomicU64,
    discovery_registrations: AtomicU64,
    max_discovery_registrations: AtomicU64,
    reservations_accepted: AtomicU64,
    reservations_denied: AtomicU64,
    circuits_accepted: AtomicU64,
    circuits_denied: AtomicU64,
    discovery_requests: AtomicU64,
    discovery_rejected: AtomicU64,
    admission_rejected: AtomicU64,
    events_dropped: AtomicU64,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            started_at_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            live: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            draining: AtomicBool::new(false),
            listeners_expected: AtomicU64::new(0),
            listeners_ready: AtomicU64::new(0),
            last_progress_ms: AtomicU64::new(0),
            connections: AtomicU64::new(0),
            max_connections: AtomicU64::new(0),
            reservations: AtomicU64::new(0),
            max_reservations: AtomicU64::new(0),
            circuits: AtomicU64::new(0),
            max_circuits: AtomicU64::new(0),
            discovery_registrations: AtomicU64::new(0),
            max_discovery_registrations: AtomicU64::new(0),
            reservations_accepted: AtomicU64::new(0),
            reservations_denied: AtomicU64::new(0),
            circuits_accepted: AtomicU64::new(0),
            circuits_denied: AtomicU64::new(0),
            discovery_requests: AtomicU64::new(0),
            discovery_rejected: AtomicU64::new(0),
            admission_rejected: AtomicU64::new(0),
            events_dropped: AtomicU64::new(0),
        }
    }
}

/// Cloneable, non-secret node health and metric state.
#[derive(Clone, Default)]
pub struct NodeStatus {
    inner: Arc<Inner>,
}

impl NodeStatus {
    pub(crate) fn start(&self) {
        self.inner.live.store(true, Ordering::Relaxed);
        self.touch();
    }

    pub(crate) fn expect_listeners(&self, listeners: usize) {
        self.inner.listeners_expected.store(
            u64::try_from(listeners).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }

    pub(crate) fn configure_capacities(
        &self,
        connections: u32,
        reservations: usize,
        circuits: usize,
        registrations: usize,
    ) {
        self.inner
            .max_connections
            .store(u64::from(connections), Ordering::Relaxed);
        self.inner.max_reservations.store(
            u64::try_from(reservations).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        self.inner.max_circuits.store(
            u64::try_from(circuits).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        self.inner.max_discovery_registrations.store(
            u64::try_from(registrations).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }

    pub(crate) fn listeners_ready(&self, listeners: usize) {
        let listeners = u64::try_from(listeners).unwrap_or(u64::MAX);
        self.inner
            .listeners_ready
            .store(listeners, Ordering::Relaxed);
        let expected = self.inner.listeners_expected.load(Ordering::Relaxed);
        self.inner
            .ready
            .store(expected > 0 && listeners >= expected, Ordering::Relaxed);
    }

    pub(crate) fn touch(&self) {
        let elapsed = self.inner.started.elapsed().as_millis();
        self.inner.last_progress_ms.store(
            u64::try_from(elapsed).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }

    pub(crate) fn begin_drain(&self) {
        self.inner.draining.store(true, Ordering::Relaxed);
        self.inner.ready.store(false, Ordering::Relaxed);
    }

    pub(crate) fn stop(&self) {
        self.inner.ready.store(false, Ordering::Relaxed);
        self.inner.live.store(false, Ordering::Relaxed);
    }

    /// Returns whether the node task is alive.
    #[must_use]
    pub fn is_live(&self) -> bool {
        let elapsed = u64::try_from(self.inner.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.is_live_at(elapsed)
    }

    fn is_live_at(&self, elapsed_ms: u64) -> bool {
        self.inner.live.load(Ordering::Relaxed)
            && elapsed_ms.saturating_sub(self.inner.last_progress_ms.load(Ordering::Relaxed))
                <= u64::try_from(LIVENESS_STALE_AFTER.as_millis()).unwrap_or(u64::MAX)
    }

    /// Returns whether every configured listener is ready and shutdown has not begun.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.inner.ready.load(Ordering::Relaxed) && !self.inner.draining.load(Ordering::Relaxed)
    }

    /// Returns the number of active transport connections.
    #[must_use]
    pub fn active_connections(&self) -> u64 {
        self.inner.connections.load(Ordering::Relaxed)
    }

    pub(crate) fn connection_opened(&self) {
        self.inner.connections.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn connection_closed(&self) {
        decrement(&self.inner.connections);
    }

    pub(crate) fn reservation_accepted(&self, renewed: bool) {
        if !renewed {
            self.inner.reservations.fetch_add(1, Ordering::Relaxed);
        }
        self.inner
            .reservations_accepted
            .fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn reservation_closed(&self) {
        decrement(&self.inner.reservations);
    }

    pub(crate) fn reservation_denied(&self) {
        self.inner
            .reservations_denied
            .fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn circuit_accepted(&self) {
        self.inner.circuits.fetch_add(1, Ordering::Relaxed);
        self.inner.circuits_accepted.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn circuit_closed(&self) {
        decrement(&self.inner.circuits);
    }

    pub(crate) fn circuit_denied(&self) {
        self.inner.circuits_denied.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn discovery_request(&self) {
        self.inner
            .discovery_requests
            .fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn discovery_rejected(&self) {
        self.inner
            .discovery_rejected
            .fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn set_discovery_registrations(&self, registrations: usize) {
        self.inner.discovery_registrations.store(
            u64::try_from(registrations).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }

    pub(crate) fn admission_rejected(&self) {
        self.inner
            .admission_rejected
            .fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn event_dropped(&self) {
        self.inner.events_dropped.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn metrics(&self) -> String {
        let value = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        format!(
            concat!(
                "# HELP envshare_node_live Whether the node task is alive.\n",
                "# TYPE envshare_node_live gauge\n",
                "envshare_node_live {}\n",
                "# HELP envshare_node_ready Whether the node accepts new traffic.\n",
                "# TYPE envshare_node_ready gauge\n",
                "envshare_node_ready {}\n",
                "# TYPE envshare_node_start_time_seconds gauge\n",
                "envshare_node_start_time_seconds {}\n",
                "# TYPE envshare_node_listeners gauge\n",
                "envshare_node_listeners {}\n",
                "# TYPE envshare_node_listener_capacity gauge\n",
                "envshare_node_listener_capacity {}\n",
                "# TYPE envshare_node_connections gauge\n",
                "envshare_node_connections {}\n",
                "# TYPE envshare_node_connection_capacity gauge\n",
                "envshare_node_connection_capacity {}\n",
                "# TYPE envshare_node_reservations gauge\n",
                "envshare_node_reservations {}\n",
                "# TYPE envshare_node_reservation_capacity gauge\n",
                "envshare_node_reservation_capacity {}\n",
                "# TYPE envshare_node_circuits gauge\n",
                "envshare_node_circuits {}\n",
                "# TYPE envshare_node_circuit_capacity gauge\n",
                "envshare_node_circuit_capacity {}\n",
                "# TYPE envshare_node_discovery_registrations gauge\n",
                "envshare_node_discovery_registrations {}\n",
                "# TYPE envshare_node_discovery_registration_capacity gauge\n",
                "envshare_node_discovery_registration_capacity {}\n",
                "# TYPE envshare_node_reservations_accepted_total counter\n",
                "envshare_node_reservations_accepted_total {}\n",
                "# TYPE envshare_node_reservations_denied_total counter\n",
                "envshare_node_reservations_denied_total {}\n",
                "# TYPE envshare_node_circuits_accepted_total counter\n",
                "envshare_node_circuits_accepted_total {}\n",
                "# TYPE envshare_node_circuits_denied_total counter\n",
                "envshare_node_circuits_denied_total {}\n",
                "# TYPE envshare_node_discovery_requests_total counter\n",
                "envshare_node_discovery_requests_total {}\n",
                "# TYPE envshare_node_discovery_rejected_total counter\n",
                "envshare_node_discovery_rejected_total {}\n",
                "# TYPE envshare_node_admission_rejected_total counter\n",
                "envshare_node_admission_rejected_total {}\n",
                "# TYPE envshare_node_events_dropped_total counter\n",
                "envshare_node_events_dropped_total {}\n",
                "# EOF\n"
            ),
            u8::from(self.is_live()),
            u8::from(self.is_ready()),
            self.inner.started_at_seconds,
            value(&self.inner.listeners_ready),
            value(&self.inner.listeners_expected),
            self.active_connections(),
            value(&self.inner.max_connections),
            value(&self.inner.reservations),
            value(&self.inner.max_reservations),
            value(&self.inner.circuits),
            value(&self.inner.max_circuits),
            value(&self.inner.discovery_registrations),
            value(&self.inner.max_discovery_registrations),
            value(&self.inner.reservations_accepted),
            value(&self.inner.reservations_denied),
            value(&self.inner.circuits_accepted),
            value(&self.inner.circuits_denied),
            value(&self.inner.discovery_requests),
            value(&self.inner.discovery_rejected),
            value(&self.inner.admission_rejected),
            value(&self.inner.events_dropped),
        )
    }
}

impl std::fmt::Debug for NodeStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NodeStatus")
            .field("live", &self.is_live())
            .field("ready", &self.is_ready())
            .finish_non_exhaustive()
    }
}

fn decrement(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_sub(1))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liveness_requires_recent_progress() {
        let status = NodeStatus::default();
        status.start();

        assert!(status.is_live_at(5_000));
        assert!(!status.is_live_at(5_001));
        status.stop();
        assert!(!status.is_live_at(0));
    }

    #[test]
    fn readiness_requires_every_expected_listener() {
        let status = NodeStatus::default();
        status.expect_listeners(2);
        status.configure_capacities(512, 128, 64, 256);
        status.start();

        status.listeners_ready(1);
        assert!(!status.is_ready());
        status.listeners_ready(2);
        assert!(status.is_ready());
        status.listeners_ready(1);
        assert!(!status.is_ready());

        status.circuit_accepted();
        let metrics = status.metrics();
        assert!(metrics.contains("envshare_node_connection_capacity 512"));
        assert!(metrics.contains("envshare_node_reservation_capacity 128"));
        assert!(metrics.contains("envshare_node_circuits 1"));
        assert!(metrics.contains("envshare_node_circuit_capacity 64"));
        assert!(metrics.contains("envshare_node_discovery_registration_capacity 256"));
        status.circuit_closed();
        assert!(status.metrics().contains("envshare_node_circuits 0"));
    }
}
