use crate::db;
use crate::models::MediaEntry;
use crate::scanner;
use crate::state::AppState;
use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn trigger_scan_handler(
    state: State<Arc<Mutex<AppState>>>,
) -> Result<Json<&'static str>, (StatusCode, String)> {
    let guard = state.0.lock().await;
    let pool = guard.pool.clone();
    let dir = guard.directory_to_scan.clone();
    drop(guard);

    scanner::scan_directory_and_index(pool, dir, None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json("Directory scan completed."))
}

#[derive(serde::Deserialize)]
pub struct ListQuery {
    pub parent_id: Option<i64>,
    // relative path to the configured media root
    pub path: Option<String>,
    // comma-separated tags e.g. tags=tag1,tag2
    pub tags: Option<String>,
    // new filters and pagination
    pub r#type: Option<String>, // "file" | "directory"
    pub kind: Option<String>,   // "image" | "video" | "audio" | "other"
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub sort: Option<String>,  // name | created | size
    pub order: Option<String>, // asc | desc
}

// --- helpers to avoid duplication ---
fn classify_kind(mime: Option<&str>) -> Option<&'static str> {
    let mt = mime.unwrap_or("");
    if mt.is_empty() {
        return None;
    }
    if mt.starts_with("image/") {
        Some("image")
    } else if mt.starts_with("video/") {
        Some("video")
    } else if mt.starts_with("audio/") {
        Some("audio")
    } else {
        Some("other")
    }
}

fn to_enriched_json(e: &MediaEntry) -> serde_json::Value {
    let mut v = serde_json::to_value(e).unwrap_or(json!({}));
    let is_dir = e.mime_type.is_none();
    if let serde_json::Value::Object(ref mut map) = v {
        // Keep thumb_path as-is; do not add URL fields here
        map.insert(
            "type".to_string(),
            serde_json::Value::String(if is_dir { "directory" } else { "file" }.to_string()),
        );
        if !is_dir {
            if let Some(k) = classify_kind(e.mime_type.as_deref()) {
                map.insert("kind".to_string(), serde_json::Value::String(k.to_string()));
            }
        }
    }
    v
}

pub async fn list_directory_handler(
    state: State<Arc<Mutex<AppState>>>,
    axum::extract::Query(q): axum::extract::Query<ListQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let guard = state.0.lock().await;
    let pool = guard.pool.clone();
    let _media_root = guard.directory_to_scan.clone();
    drop(guard);

    // parse tags into Vec<String>
    let tags_vec: Option<Vec<String>> = q.tags.as_ref().map(|s| {
        s.split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect()
    });

    if let Some(rel_path) = q.path {
        // validate relative path: must not start with '/' and must not contain '..'
        if rel_path.starts_with('/') || rel_path.contains("..") {
            return Err((
                StatusCode::BAD_REQUEST,
                "path must be relative to the media root".to_string(),
            ));
        }

        // find the media entry for this path
        let opt = db::get_media_by_path(pool.clone(), rel_path.clone())
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        match opt {
            Some(entry) => {
                // if directory (mime_type is None), list its children
                if entry.mime_type.is_none() {
                    let rows = db::list_children_advanced(
                        pool.clone(),
                        Some(entry.id),
                        tags_vec,
                        q.r#type.as_deref(),
                        q.kind.as_deref(),
                        q.limit,
                        q.offset,
                        q.sort.as_deref(),
                        q.order.as_deref(),
                    )
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                    let enriched: Vec<serde_json::Value> =
                        rows.into_iter().map(|e| to_enriched_json(&e)).collect();
                    Ok(Json(json!({ "files": enriched })))
                } else {
                    // file: return single enriched entry
                    let v = to_enriched_json(&entry);
                    Ok(Json(json!({ "files": [v] })))
                }
            }
            _ => Err((StatusCode::NOT_FOUND, "Path not found".to_string())),
        }
    } else {
        // use parent_id (may be None) to list children
        let rows = db::list_children_advanced(
            pool.clone(),
            q.parent_id,
            tags_vec,
            q.r#type.as_deref(),
            q.kind.as_deref(),
            q.limit,
            q.offset,
            q.sort.as_deref(),
            q.order.as_deref(),
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        // enrich with type/kind and URLs
        let enriched: Vec<serde_json::Value> =
            rows.into_iter().map(|e| to_enriched_json(&e)).collect();
        Ok(Json(json!({ "files": enriched })))
    }
}

#[derive(serde::Deserialize)]
pub struct DetailsQuery {
    pub path: Option<String>,
}

pub async fn get_file_details_handler(
    state: State<Arc<Mutex<AppState>>>,
    axum::extract::Query(q): axum::extract::Query<DetailsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let key = q.path.clone().unwrap_or_default();
    let guard = state.0.lock().await;
    let pool = guard.pool.clone();
    let _media_root = guard.directory_to_scan.clone();
    drop(guard);

    // try id then path
    if let Ok(id) = key.parse::<i64>() {
        let opt = db::get_media_by_id(pool, id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        match opt {
            Some(entry) => {
                let val = to_enriched_json(&entry);
                Ok(Json(val))
            }
            _ => Err((StatusCode::NOT_FOUND, "File not found".to_string())),
        }
    } else {
        // validate relative path
        if key.starts_with('/') || key.contains("..") {
            return Err((
                StatusCode::BAD_REQUEST,
                "path must be relative to the media root".to_string(),
            ));
        }

        let opt = db::get_media_by_path(pool, key)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        match opt {
            Some(entry) => Ok(Json(to_enriched_json(&entry))),
            _ => Err((StatusCode::NOT_FOUND, "File not found".to_string())),
        }
    }
}
