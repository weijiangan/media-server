use serde::Deserialize;

#[derive(Deserialize)]
pub struct AppConfig {
    pub db_path: String,
    pub directory_to_scan: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub ffmpeg_enabled: Option<bool>,
    pub ffmpeg_path: Option<String>,
    pub ffprobe_path: Option<String>,
    pub thumbnails_dir: Option<String>,
    pub cors_allowed_origins: Option<Vec<String>>,
    pub cors_allow_credentials: Option<bool>,
}
