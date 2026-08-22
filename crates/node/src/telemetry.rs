//! Low-cardinality in-process node health and metrics.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

#[derive(Default)]
struct Inner {
    live: AtomicBool,
    ready: AtomicBool,
    draining: AtomicBool,
    connections: AtomicU64,
    reservations: AtomicU64,
    discovery_registrations: AtomicU64,
    reservations_accepted: AtomicU64,
    reservations_denied: AtomicU64,
    circuits_accepted: AtomicU64,
    circuits_denied: AtomicU64,
    discovery_requests: AtomicU64,
    discovery_rejected: AtomicU64,
    admission_rejected: AtomicU64,
    events_dropped: AtomicU64,
}

/// Cloneable, non-secret node health and metric state.
#[derive(Clone, Default)]
pub struct NodeStatus {
    inner: Arc<Inner>,
}

impl NodeStatus {
    pub(crate) fn start(&self) {
        self.inner.live.store(true, Ordering::Relaxed);
    }

    pub(crate) fn listening(&self) {
        self.inner.ready.store(true, Ordering::Relaxed);
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
        self.inner.live.load(Ordering::Relaxed)
    }

    /// Returns whether at least one listener is ready and shutdown has not begun.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.inner.ready.load(Ordering::Relaxed) && !self.inner.draining.load(Ordering::Relaxed)
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
        self.inner.circuits_accepted.fetch_add(1, Ordering::Relaxed);
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
                "# TYPE envshare_node_connections gauge\n",
                "envshare_node_connections {}\n",
                "# TYPE envshare_node_reservations gauge\n",
                "envshare_node_reservations {}\n",
                "# TYPE envshare_node_discovery_registrations gauge\n",
                "envshare_node_discovery_registrations {}\n",
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
            value(&self.inner.connections),
            value(&self.inner.reservations),
            value(&self.inner.discovery_registrations),
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
