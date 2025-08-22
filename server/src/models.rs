use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MediaEntry {
    pub id: i64,
    pub name: String,
    // path is stored relative to the configured media root
    pub path: String,
    pub parent_id: Option<i64>,
    pub mime_type: Option<String>,
    pub size: Option<i64>,
    pub created_at: String,
    // optional list of tags
    pub tags: Option<Vec<String>>,
    // optional thumbnail path (relative to server directory e.g. .thumbnails/<id>.jpg)
    pub thumb_path: Option<String>,
    // optional dimensions for images
    pub width: Option<i64>,
    pub height: Option<i64>,
    // optional duration (seconds) for videos
    pub duration_secs: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NewMediaEntry {
    pub name: String,
    // relative path
    pub path: String,
    pub parent_id: Option<i64>,
    pub mime_type: Option<String>,
    pub size: Option<i64>,
    // optional tags when creating/updating
    pub tags: Option<Vec<String>>,
    // optional thumbnail path and metadata when known
    pub thumb_path: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_secs: Option<i64>,
}
