# Task 04 — Transport: HTTP

## Objective
Fully specify the HTTP transport implementation for alphastream-rs, integrating with the Transport trait.

## Client Setup
Use reqwest async client with TLS defaults, connection pooling, and builder-configurable options.

```rust
use reqwest::Client;

let client = Client::builder()
    .tls_built_in_root_certs(true)
    .pool_max_idle_per_host(10)
    .timeout(Duration::from_secs(10))
    .build()?;
```

## Range Requests
Implement HTTP Range header usage for chunked downloads. Chunk size: $1\,\text{MiB}$. Max concurrent ranges: 4.

Range header format: "Range: bytes=START-END"

```rust
let range_header = format!("bytes={}-{}", start, end);
let response = client.get(url)
    .header("Range", range_header)
    .send()
    .await?;
```

## Concurrency & Backpressure
Manage concurrent fetches using a semaphore with max_concurrent_ranges=4. Integrate backpressure with scheduler via async channels.

```rust
use tokio::sync::Semaphore;

let semaphore = Arc::new(Semaphore::new(4));
// In fetch loop:
let _permit = semaphore.acquire().await?;
```

## Retries & Timeouts
Exponential backoff starting at $250\,\text{ms}$, retries=3, per-request timeout $10\,\text{s}$.

Backoff formula: $delay = 250 \times 2^{attempt - 1}$

```rust
use tokio::time::{sleep, Duration};

for attempt in 0..3 {
    match client.get(url).send().await {
        Ok(resp) => return Ok(resp),
        Err(_) => {
            let delay = Duration::from_millis(250 * (1 << attempt));
            sleep(delay).await;
        }
    }
}
```

## Error Mapping
Map HTTP errors to TransportError.

- 404 → NotFound
- Timeout → Timeout
- 5xx → ServerError
- Other → GenericError

## Implementation Checklist
- [ ] Create reqwest client with builder options
- [ ] Implement range fetch logic with chunk_size=$1\,\text{MiB}$
- [ ] Add concurrency control with semaphore (max=4)
- [ ] Implement retry loop with exponential backoff
- [ ] Map HTTP status codes to TransportError variants
- [ ] Integrate with Transport trait

## Acceptance Criteria
- Integration test against a static server verifies range requests and chunking
- Throughput metrics exceed baseline; error paths verified for 404, timeout, 5xx

## References
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
- [docs/tasks/03-transport-abstraction.md](docs/tasks/03-transport-abstraction.md)
