use serde::Deserialize;

#[derive(Deserialize)]
pub struct AppConfig {
    pub db_path: String,
    pub directory_to_scan: String,
    pub host: Option<String>,
    pub port: Option<u16>,
}
