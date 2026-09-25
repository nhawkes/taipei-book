---
title: What is taipei
---

[Taipei](https://github.com/nhawkes/taipei) is a library that integrates with [Tower](https://github.com/tower-rs/tower) to enable writing servers that behave well under stress without tuning.

The examples here assume some familiarity with [Tokio](https://tokio.rs/) and [Tower](https://github.com/tower-rs/tower). Tokio is the most commonly used runtime for writing asynchronous Rust programs. It is often used in servers, allowing a single CPU thread to handle multiple requests and connections. Tower provides a `Service` trait that acts as an interface for composing reliability abstractions. For example, a Tower middleware layer might add retries. Taipei's functionality is implemented as Tower layers. That is, we take your service as a `my_service: impl Service` and wrap it with additional functionality to create `my_reliable_service: impl Service`. Whilst the examples use Tokio, they could be adapted to work with other runtimes, and the same principles can be applied in any language.

Here is a full example of a reliable server:
```rust
use axum::{error_handling::HandleErrorLayer, routing::get, Router};
use http::StatusCode;
use taipei::backpressure::{CpuBackpressureLayer, InstrumentedRuntime as _};
use taipei::queue::{QueueError, QueueLayer};
use taipei::tokio::InstrumentedTokioRuntime;
use tower::{make::Shared, ServiceBuilder};

fn main() -> anyhow::Result<()> {
    // instrument tokio's CPU usage
    let instrumented = InstrumentedTokioRuntime::new()?;
    let instr = instrumented.instrumentation();
    let handle = instrumented.runtime.handle().clone();

    let my_service = Router::new().route("/", get(|| async { "hello" }));

    // hold requests while CPU usage is above 50%
    let inner = ServiceBuilder::new()
        .layer(CpuBackpressureLayer::new(&instr))
        .service(my_service);
    let (service, worker) = QueueLayer::new().build(inner, handle.clone());

    // handle timeout in queue and other errors
    let service = Shared::new(
        ServiceBuilder::new()
            .layer(HandleErrorLayer::new(|e: QueueError| async move {
                (StatusCode::SERVICE_UNAVAILABLE, e.to_string())
            }))
            .service(service),
    );

    instrumented.runtime.block_on(async move {
        handle.spawn(worker.serve());
        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
        axum::serve(listener, service).await?;
        Ok(())
    })
}
```
And a full visualization

```sim
{ "sim": "queue-viz", "width": 960, "height": 520, "stage": "queue" }
```

We'll walk through why each component exists step-by-step and how we can do better than a manually tuned concurrency limit.