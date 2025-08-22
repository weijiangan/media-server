use crate::db;
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
                    let rows = db::list_children(pool, Some(entry.id), tags_vec)
                        .await
                        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                    // enrich rows with thumbnail_url and stream_url
                    let enriched: Vec<serde_json::Value> = rows
                        .into_iter()
                        .map(|e| {
                            // default thumbnail size
                            let thumb = e
                                .thumb_path
                                .clone()
                                .map(|_| format!("/thumbnails/{}_200x200.jpg", e.id));
                            let stream =
                                if e.mime_type.as_deref().unwrap_or("").starts_with("image/") {
                                    format!("/media/image?path={}", e.path)
                                } else {
                                    format!("/media/stream?path={}", e.path)
                                };
                            let mut v = serde_json::to_value(e).unwrap_or(json!({}));
                            if let serde_json::Value::Object(ref mut map) = v {
                                if let Some(t) = thumb {
                                    map.insert(
                                        "thumbnail_url".to_string(),
                                        serde_json::Value::String(t),
                                    );
                                }
                                map.insert(
                                    "stream_url".to_string(),
                                    serde_json::Value::String(stream),
                                );
                            }
                            v
                        })
                        .collect();
                    Ok(Json(json!({ "files": enriched })))
                } else {
                    // file: return single enriched entry
                    let v = serde_json::to_value(&entry).unwrap_or(json!({}));
                    let v = if let serde_json::Value::Object(mut map) = v {
                        if entry.thumb_path.is_some() {
                            map.insert(
                                "thumbnail_url".to_string(),
                                serde_json::Value::String(format!(
                                    "/thumbnails/{}_200x200.jpg",
                                    entry.id
                                )),
                            );
                        }
                        let stream = if entry
                            .mime_type
                            .as_deref()
                            .unwrap_or("")
                            .starts_with("image/")
                        {
                            format!("/media/image?path={}", entry.path)
                        } else {
                            format!("/media/stream?path={}", entry.path)
                        };
                        map.insert("stream_url".to_string(), serde_json::Value::String(stream));
                        serde_json::Value::Object(map)
                    } else {
                        v
                    };
                    Ok(Json(json!({ "files": [v] })))
                }
            }
            _ => Err((StatusCode::NOT_FOUND, "Path not found".to_string())),
        }
    } else {
        // use parent_id (may be None) to list children
        let rows = db::list_children(pool, q.parent_id, tags_vec)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(Json(json!({ "files": rows })))
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
                // add thumbnail and stream URLs
                let thumb_url = entry
                    .thumb_path
                    .clone()
                    .map(|_| format!("/media/thumbnail?path={}", entry.path));
                let stream_or_image = if entry
                    .mime_type
                    .as_deref()
                    .unwrap_or("")
                    .starts_with("image/")
                {
                    format!("/media/image?path={}", entry.path)
                } else {
                    format!("/media/stream?path={}", entry.path)
                };
                let mut val = serde_json::to_value(entry).unwrap_or(json!({}));
                if let serde_json::Value::Object(ref mut map) = val {
                    if let Some(tu) = thumb_url {
                        map.insert("thumbnail_url".to_string(), serde_json::Value::String(tu));
                    }
                    map.insert(
                        "stream_url".to_string(),
                        serde_json::Value::String(stream_or_image),
                    );
                }
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
            Some(entry) => {
                let thumb_url = entry
                    .thumb_path
                    .clone()
                    .map(|_| format!("/media/thumbnail?path={}", entry.path));
                let stream_or_image = if entry
                    .mime_type
                    .as_deref()
                    .unwrap_or("")
                    .starts_with("image/")
                {
                    format!("/media/image?path={}", entry.path)
                } else {
                    format!("/media/stream?path={}", entry.path)
                };
                let mut val = serde_json::to_value(entry).unwrap_or(json!({}));
                if let serde_json::Value::Object(ref mut map) = val {
                    if let Some(tu) = thumb_url {
                        map.insert("thumbnail_url".to_string(), serde_json::Value::String(tu));
                    }
                    map.insert(
                        "stream_url".to_string(),
                        serde_json::Value::String(stream_or_image),
                    );
                }
                Ok(Json(val))
            }
            _ => Err((StatusCode::NOT_FOUND, "File not found".to_string())),
        }
    }
}
