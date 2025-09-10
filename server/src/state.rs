use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::sync::{Mutex, Semaphore};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub directory_to_scan: String,
    pub ffmpeg_enabled: bool,
    pub ffmpeg_path: Option<String>,
    pub ffprobe_path: Option<String>,
    pub thumbnails_dir: Option<String>,
    pub client_dist_dir: Option<String>,
    // Regeneration controls
    pub regen_semaphore: Arc<Semaphore>,
    // Track in-flight keys mapping to waiters so concurrent callers can wait for
    // completion instead of spawning duplicate work. Key -> Vec<oneshot::Sender<Result<(),String>>>
    pub in_flight: Arc<Mutex<HashMap<String, Vec<oneshot::Sender<Result<(), String>>>>>>,
}
