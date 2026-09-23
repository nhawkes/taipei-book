//! The protection compositions the queue visualiser runs — **and shows**.
//!
//! Each function is the exact tower stack for one stage, its body captured verbatim by
//! `#[shown]` into a `*_SRC` constant. The engine calls the function to build the
//! service; the panel displays the constant. The code the reader sees is the code that
//! ran, because it is the same function — there is no second, hand-copied listing to
//! drift from it.
//!
//! `my_service` is the sim's modelled server. `handle` is the runtime the queue workers spawn
//! on. `limit` is the concurrency ceiling the slider sets — moving it rebuilds the stack
//! and hot-swaps it in (see [`crate::hotswap`]), so a plain composition needs no live
//! handle and its body stays pristine. The one exception is the OS-CPU gate, whose whole
//! point is a handle a controller tunes; there the handle *is* the mechanism, and the
//! listing shows it.

use std::sync::Arc;
use std::time::Duration;

use blog_core::PolicyStage;
use taipei::backpressure::{CpuBackpressureLayer, CpuBackpressureService, RuntimeInstrumentation};
use taipei::limit::{
    ConcurrencyLimit, DynamicConcurrencyLimitLayer, DynamicConcurrencyLimitService,
};
use taipei::queue::{QueueLayer, QueueService, QueueWorker};
use taipei::rate_limit::{EnforcerLayer, EnforcerService, Limits};
use taipei::reject::{RejectionLayer, RejectionService};
use taipei::tenant::{Report, TenantReportService, TenantReporter};
use tokio::runtime::Handle;
use tower::{Layer, ServiceBuilder};

use crate::engine::{App, SimReq};
use crate::unbounded_queue::{UnboundedQueue, UnboundedQueueService, UnboundedQueueWorker};

/// CPU backpressure only: readiness is withheld while every core is busy, so a caller
/// waits for one. Nothing sheds — there is no escape valve yet.
#[taipei_macros::shown(BACKPRESSURE_SRC)]
pub(crate) fn backpressure(
    my_service: App,
    instr: &RuntimeInstrumentation,
) -> CpuBackpressureService<App> {
    ServiceBuilder::new()
        .layer(CpuBackpressureLayer::new(instr))
        .service(my_service)
}

/// A concurrency limit plus immediate rejection, no queue.
#[taipei_macros::shown(REJECT_SRC)]
pub(crate) fn reject(
    my_service: App,
    limit: usize,
) -> RejectionService<DynamicConcurrencyLimitService<App>> {
    // Reject if too many requests
    ServiceBuilder::new()
        .layer(RejectionLayer::new())
        .layer(DynamicConcurrencyLimitLayer::new(limit))
        .service(my_service)
}

/// The same limit with nothing above it: callers wait for a slot, unboundedly.
#[taipei_macros::shown(WAIT_SRC)]
pub(crate) fn wait(
    my_service: App,
    limit: usize,
    handle: Handle,
) -> (
    UnboundedQueueService<SimReq, ()>,
    UnboundedQueueWorker<DynamicConcurrencyLimitService<App>, SimReq, ()>,
) {
    // Queue if too many requests
    let inner = ServiceBuilder::new()
        .layer(DynamicConcurrencyLimitLayer::new(limit))
        .service(my_service);
    UnboundedQueue::build(inner, handle)
}

/// A hand-tuned concurrency limit behind the queue — the naive gate.
#[taipei_macros::shown(QUEUE_NAIVE_SRC)]
pub(crate) fn queue_naive(
    my_service: App,
    limit: usize,
    handle: Handle,
    queue_timeout: Duration,
) -> (
    QueueService<SimReq, ()>,
    QueueWorker<DynamicConcurrencyLimitService<App>, SimReq, ()>,
) {
    // A manually picked limit (tune below)
    let inner = ServiceBuilder::new()
        .layer(DynamicConcurrencyLimitLayer::new(limit))
        .service(my_service);
    QueueLayer::new(queue_timeout).build(inner, handle)
}

/// CPU backpressure behind the queue — the gate taipei recommends.
#[taipei_macros::shown(QUEUE_SRC)]
pub(crate) fn queue(
    my_service: App,
    instr: &RuntimeInstrumentation,
    handle: Handle,
    queue_timeout: Duration,
) -> (
    QueueService<SimReq, ()>,
    QueueWorker<CpuBackpressureService<App>, SimReq, ()>,
) {
    // Queue until tokio has 50% of cores available
    let inner = ServiceBuilder::new()
        .layer(CpuBackpressureLayer::new(instr))
        .service(my_service);
    QueueLayer::new(queue_timeout).build(inner, handle)
}

/// The recommended gate with tenant accounting under it. The reporter sits **below the queue and
/// above the gate**, which is the only place it can measure: `poll_ready` is where a caller is told
/// to wait, so the layer's own `poll_ready` — delegating to the backpressure gate's — spans exactly
/// the shut-time, and the queue worker racing its deadline on `service.ready()` is the one caller
/// the reporter is specified for.
#[taipei_macros::shown(QUEUE_TENANT_SRC)]
pub(crate) fn queue_tenant<R: Report + Send + 'static>(
    my_service: App,
    instr: &RuntimeInstrumentation,
    handle: Handle,
    queue_timeout: Duration,
    reporter: &TenantReporter<R>,
) -> (
    QueueService<SimReq, ()>,
    QueueWorker<TenantReportService<CpuBackpressureService<App>, R>, SimReq, ()>,
) {
    let inner = ServiceBuilder::new()
        .layer(reporter)
        .layer(CpuBackpressureLayer::new(instr))
        .service(my_service);
    QueueLayer::new(queue_timeout).build(inner, handle)
}

/// The tenanted gate, with the fleet's rate limit in front of it. The one composition where a
/// server is not deciding alone: `limits` is the shared store, and both directions of it are
/// here — blame goes out through the reporter, and the share to refuse comes back through the
/// layer on top.
///
/// The limit is the **outermost** layer, above the queue. A request that is going to be refused
/// must not first take a queue slot from one that is not.
#[taipei_macros::shown(QUEUE_RATE_LIMITED_SRC)]
pub(crate) fn queue_rate_limited<L, R>(
    my_service: App,
    instr: &RuntimeInstrumentation,
    handle: Handle,
    queue_timeout: Duration,
    reporter: &TenantReporter<R>,
    limits: Arc<L>,
) -> (
    EnforcerService<QueueService<SimReq, ()>, L>,
    QueueWorker<TenantReportService<CpuBackpressureService<App>, R>, SimReq, ()>,
)
where
    L: Limits,
    R: Report + Send + 'static,
{
    let inner = ServiceBuilder::new()
        .layer(reporter)
        .layer(CpuBackpressureLayer::new(instr))
        .service(my_service);
    let (queue, worker) = QueueLayer::new(queue_timeout).build(inner, handle);
    (EnforcerLayer::new(limits).layer(queue), worker)
}

/// An OS-CPU controller tuning the limit behind the queue. The controller drives the
/// shared [`ConcurrencyLimit`] from a load signal (the sim feeds it utilisation per
/// frame where a server would read the cgroup); admission stays instant, only the
/// ceiling moves — slowly, which is the lesson.
#[taipei_macros::shown(QUEUE_OS_CPU_SRC)]
pub(crate) fn queue_os_cpu(
    my_service: App,
    limit: ConcurrencyLimit,
    handle: Handle,
    queue_timeout: Duration,
) -> (
    QueueService<SimReq, ()>,
    QueueWorker<DynamicConcurrencyLimitService<App>, SimReq, ()>,
) {
    // loop { sleep(3secs); update_from_os_cpu(&limit); }
    let inner = ServiceBuilder::new()
        .layer(DynamicConcurrencyLimitLayer::from_handle(limit))
        .service(my_service);
    QueueLayer::new(queue_timeout).build(inner, handle)
}

/// The captured source of the composition [`crate::engine`] runs for a stage — the exact
/// text of the function above, so the panel can never show code the machine isn't running.
/// The bare stage composes nothing, so it has nothing to show.
pub(crate) fn src_for(
    stage: PolicyStage,
    gate: Option<crate::engine::Gate>,
) -> Option<&'static str> {
    use crate::engine::Gate;
    match (stage, gate) {
        (PolicyStage::App, _) => None,
        (PolicyStage::Backpressure, _) => Some(BACKPRESSURE_SRC),
        (PolicyStage::Reject, _) => Some(REJECT_SRC),
        (PolicyStage::Wait, _) => Some(WAIT_SRC),
        (PolicyStage::Queue, Some(Gate::RuntimeCpu)) => Some(QUEUE_SRC),
        (PolicyStage::Queue, Some(Gate::OsCpu)) => Some(QUEUE_OS_CPU_SRC),
        (PolicyStage::Queue, _) => Some(QUEUE_NAIVE_SRC),
    }
}

/// The ceiling [`PolicyStage::Reject`] admits before it sheds — one slot per simulated
/// core, held for the whole request, so a saturated CPU is a full limit.
pub const REJECT_LIMIT: usize = 8;

/// The ceiling [`PolicyStage::Queue`] admits behind its queue. A slot is held across the
/// request's IO as well as its CPU, so a per-core ceiling would leave cores idle
/// waiting on the database; this is tuned to the in-flight population those cores can
/// actually sustain — and being tuned by hand is exactly what makes it naive.
pub const QUEUE_LIMIT: usize = 24;

/// Which protection layers a stage composes. Present ⟺ drawn; this is what the
/// visualiser inspects so the animation matches the running stack exactly.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Layers {
    /// CPU backpressure gates admission (withholds readiness, never rejects).
    pub backpressure: bool,
    /// A concurrency limit caps in-flight requests; `Some(n)` is the ceiling.
    pub limit: Option<usize>,
    /// A rejection layer sheds immediately when the inner service isn't ready.
    pub reject: bool,
    /// A **request queue**: accepted requests, parked awaiting admission. Distinct from
    /// the kernel's SYN backlog (those are not accepted yet) and from the runtime's run
    /// queue (those are runnable now, not waiting for permission). Any composition that
    /// parks a caller has one — a bare concurrency limit's semaphore waiters are a
    /// request queue too, just an implicit one nobody chose the shape of.
    pub queue: bool,
    /// The queue's shed deadline. `None` is the unbounded wait you get when nothing
    /// puts a clock on it — which is what [`QueueLayer`] exists to fix.
    pub queue_timeout: Option<Duration>,
}

impl Layers {
    /// The layer set a stage composes — the source of truth for the viz.
    pub fn of(stage: PolicyStage, queue_timeout: Duration) -> Self {
        match stage {
            PolicyStage::App => Layers::default(),
            PolicyStage::Backpressure => Layers {
                backpressure: true,
                ..Layers::default()
            },
            PolicyStage::Reject => Layers {
                limit: Some(REJECT_LIMIT),
                reject: true,
                ..Layers::default()
            },
            // The same ceiling as Reject and no rejection, so reaching it is a wait —
            // and a wait is a request queue, here the implicit one the limit's own
            // semaphore keeps. No deadline is on it, which is the whole difference.
            PolicyStage::Wait => Layers {
                limit: Some(REJECT_LIMIT),
                queue: true,
                ..Layers::default()
            },
            PolicyStage::Queue => Layers {
                limit: Some(QUEUE_LIMIT),
                queue: true,
                queue_timeout: Some(queue_timeout),
                ..Layers::default()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Duration = Duration::from_millis(100);

    fn stages() -> [PolicyStage; 5] {
        use PolicyStage::*;
        let all = [App, Backpressure, Reject, Wait, Queue];
        // A new variant makes this match non-exhaustive, so the list cannot fall behind
        // the enum and quietly shrink what the tests below range over.
        all.map(|stage| match stage {
            App | Backpressure | Reject | Wait | Queue => stage,
        })
    }

    /// The gate is its own chapter, and the stage the book reaches first must not have
    /// arrived there already: `Queue` is the naive ceiling, tuned by hand. Backpressure
    /// is composed onto a manifest afterwards, so no stage may bring it along.
    #[test]
    fn backpressure_belongs_to_exactly_one_stage() {
        let carried: Vec<PolicyStage> = stages()
            .into_iter()
            .filter(|&s| Layers::of(s, T).backpressure)
            .collect();
        assert_eq!(carried, [PolicyStage::Backpressure]);
    }

    /// The comparison those two pages are built on: the same ceiling, opposite answers
    /// at it. Let the ceilings diverge and the pages stop comparing like with like.
    #[test]
    fn wait_and_reject_differ_only_in_what_reaching_the_ceiling_does() {
        let (wait, reject) = (
            Layers::of(PolicyStage::Wait, T),
            Layers::of(PolicyStage::Reject, T),
        );
        assert_eq!(wait.limit, reject.limit);
        assert!(reject.reject && !reject.queue, "reaching it sheds");
        assert!(wait.queue && !wait.reject, "reaching it waits");
    }

    /// A deadline is what separates the two waits, it is meaningless without something
    /// to wait in, and it is the caller's — a constant here would draw one clock while
    /// the tower ran another.
    #[test]
    fn the_deadline_is_the_callers_and_belongs_to_a_queue() {
        for stage in stages() {
            let layers = Layers::of(stage, T);
            assert!(
                layers.queue_timeout.is_none() || layers.queue,
                "{stage:?} times out nothing"
            );
        }
        assert!(
            Layers::of(PolicyStage::Wait, T).queue_timeout.is_none(),
            "the unbounded wait"
        );
        assert!(
            Layers::of(PolicyStage::Queue, T).queue_timeout.is_some(),
            "the deadline that ends it"
        );
        assert_ne!(
            Layers::of(PolicyStage::Queue, T).queue_timeout,
            Layers::of(PolicyStage::Queue, T * 2).queue_timeout
        );
    }

    /// Present ⟺ drawn. Two stages sharing a manifest are one picture, and the reader
    /// would be looking at a composition other than the one the page names.
    #[test]
    fn each_stage_draws_a_distinct_picture() {
        for (i, a) in stages().into_iter().enumerate() {
            for b in stages().into_iter().skip(i + 1) {
                assert_ne!(
                    Layers::of(a, T),
                    Layers::of(b, T),
                    "{a:?} and {b:?} draw the same picture"
                );
            }
        }
    }
}
