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
}
