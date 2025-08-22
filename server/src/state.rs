use sqlx::SqlitePool;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub directory_to_scan: String,
    pub ffmpeg_enabled: bool,
    pub ffmpeg_path: Option<String>,
    pub ffprobe_path: Option<String>,
    pub thumbnails_dir: Option<String>,
}
