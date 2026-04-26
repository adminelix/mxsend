use std::time::Duration;

/// Manages a background sync loop for a Matrix client on a separate thread.
///
/// Matrix clients need to continuously sync to receive events. This helper
/// spawns a sync loop on a dedicated thread so tests can run sender logic
/// on the main async runtime while the receiver listens in the background.
#[derive(Debug)]
pub struct SyncThread {
    handle: Option<std::thread::JoinHandle<()>>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl SyncThread {
    /// Starts a sync loop for the given client.
    ///
    /// The loop runs on a new thread with its own tokio runtime.
    /// Call `stop()` to gracefully shut down.
    pub fn start(client: matrix_sdk::Client) -> Self {
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create runtime");

            rt.block_on(async move {
                loop {
                    tokio::select! {
                        _ = &mut shutdown_rx => break,
                        result = client.sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(2))) => {
                            if result.is_err() {
                                break;
                            }
                        }
                    }
                }
            });
        });

        Self {
            handle: Some(handle),
            shutdown: Some(shutdown_tx),
        }
    }

    /// Gracefully stops the sync loop and joins the thread.
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for SyncThread {
    fn drop(&mut self) {
        self.stop();
    }
}
