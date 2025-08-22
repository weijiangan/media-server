use crate::db;
use crate::state::AppState;
use axum::body::StreamBody;
use axum::http::{HeaderValue, Request, StatusCode as AxumStatusCode};
use axum::response::Response;
use axum::{extract::State, http::StatusCode};
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncSeekExt;
use tokio::sync::Mutex;
use tokio_util::io::ReaderStream;

#[derive(serde::Deserialize)]
pub struct StreamQuery {
    pub id: Option<i64>,
    pub path: Option<String>,
}

pub async fn stream_handler(
    state: State<Arc<Mutex<AppState>>>,
    axum::extract::Query(q): axum::extract::Query<StreamQuery>,
    req: Request<axum::body::Body>,
) -> Result<Response, (StatusCode, String)> {
    let guard = state.0.lock().await;
    let pool = guard.pool.clone();
    let media_root = guard.directory_to_scan.clone();
    drop(guard);

    // Locate entry by id or path
    let opt = if let Some(id) = q.id {
        db::get_media_by_id(pool.clone(), id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else if let Some(p) = q.path.clone() {
        if p.starts_with('/') || p.contains("..") {
            return Err((StatusCode::BAD_REQUEST, "path must be relative".to_string()));
        }
        db::get_media_by_path(pool.clone(), p)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        None
    };

    let entry = match opt {
        Some(e) => e,
        None => return Err((StatusCode::NOT_FOUND, "Not found".to_string())),
    };

    let file_path = Path::new(&media_root).join(&entry.path);
    let meta = tokio::fs::metadata(&file_path)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let total_size = meta.len();

    // Parse and validate Range header (single range only)
    let (range_start, range_end, is_partial) = if let Some(hv) = req.headers().get("range") {
        if let Ok(s) = hv.to_str() {
            if s.starts_with("bytes=") {
                let rest = &s[6..];
                let parts: Vec<&str> = rest.split('-').collect();
                if parts.len() == 2 {
                    let start_opt = if !parts[0].is_empty() {
                        parts[0].parse::<u64>().ok()
                    } else {
                        None
                    };
                    let end_opt = if !parts[1].is_empty() {
                        parts[1].parse::<u64>().ok()
                    } else {
                        None
                    };
                    match (start_opt, end_opt) {
                        (Some(start), Some(end)) if start <= end && end < total_size => {
                            (start, end, true)
                        }
                        (Some(start), None) if start < total_size => (start, total_size - 1, true),
                        (None, Some(end)) if end != 0 && end < total_size => {
                            (total_size - end, total_size - 1, true)
                        }
                        _ => {
                            // Invalid range
                            return Err((
                                StatusCode::RANGE_NOT_SATISFIABLE,
                                format!("Invalid Range: {}", s),
                            ));
                        }
                    }
                } else {
                    // Malformed range
                    return Err((
                        StatusCode::RANGE_NOT_SATISFIABLE,
                        format!("Malformed Range: {}", s),
                    ));
                }
            } else {
                // Not a bytes range
                (0, total_size - 1, false)
            }
        } else {
            (0, total_size - 1, false)
        }
    } else {
        (0, total_size - 1, false)
    };

    let length = range_end.saturating_sub(range_start) + 1;
    let file = tokio::fs::File::open(&file_path)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut reader = tokio::io::BufReader::new(file);
    reader
        .seek(std::io::SeekFrom::Start(range_start))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    use tokio::io::AsyncReadExt;
    let limited = reader.take(length);
    let stream = ReaderStream::new(limited);
    let body = StreamBody::new(stream);
    let boxed = axum::body::boxed(body);
    let mut res = Response::new(boxed);
    res.headers_mut()
        .insert("Accept-Ranges", HeaderValue::from_static("bytes"));
    let ctype = entry
        .mime_type
        .clone()
        .unwrap_or_else(|| "application/octet-stream".to_string());
    res.headers_mut().insert(
        "content-type",
        HeaderValue::from_str(&ctype)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    res.headers_mut().insert(
        "Content-Length",
        HeaderValue::from_str(&length.to_string()).unwrap(),
    );
    if is_partial {
        *res.status_mut() = AxumStatusCode::PARTIAL_CONTENT;
        let content_range = format!("bytes {}-{}/{}", range_start, range_end, total_size);
        res.headers_mut().insert(
            "Content-Range",
            HeaderValue::from_str(&content_range).unwrap(),
        );
    }
    Ok(res)
}
