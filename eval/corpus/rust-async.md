# Async Rust with Tokio

An `async fn` returns a future that does nothing until it is polled. The Tokio runtime polls futures on a pool of worker threads and wakes them when I/O is ready.

Never block a runtime thread: CPU-heavy work or blocking calls (file parsing, compression, model inference) belong in `tokio::task::spawn_blocking`, which runs them on a separate thread pool.

Futures spawned with `tokio::spawn` must be `Send` because they may move between threads; holding a `std::sync::MutexGuard` across an `.await` breaks that. Use `tokio::join!` to run futures concurrently and `tokio::select!` to race them, for example against a shutdown signal or a timeout.
