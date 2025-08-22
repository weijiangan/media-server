use crate::db;
use crate::models;
use crate::state::AppState;
use axum::body::StreamBody;
use axum::http::HeaderValue;
use axum::response::{IntoResponse, Redirect, Response};
use axum::{extract::State, http::StatusCode};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::File;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio_util::io::ReaderStream;

#[derive(serde::Deserialize)]
pub struct ThumbQuery {
    pub id: Option<i64>,
    pub path: Option<String>,
    pub w: Option<u32>,
    pub h: Option<u32>,
}

#[derive(serde::Deserialize)]
pub struct GenThumbQuery {
    pub id: Option<i64>,
    pub path: Option<String>,
    pub w: Option<u32>,
    pub h: Option<u32>,
}

pub async fn thumbnail_handler(
    state: State<Arc<Mutex<AppState>>>,
    axum::extract::Query(q): axum::extract::Query<ThumbQuery>,
) -> Result<Response, (StatusCode, String)> {
    let guard = state.0.lock().await;
    let pool = guard.pool.clone();
    let _media_root = guard.directory_to_scan.clone();
    // resolve thumbnails dir from state (must be provided by main)
    let thumbs_dir = guard
        .thumbnails_dir
        .clone()
        .map(PathBuf::from)
        .expect("thumbnails_dir must be configured in AppState");
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

    // If thumbnail exists on disk, serve it. Otherwise, try to generate for images.
    let _ = tokio::fs::create_dir_all(&thumbs_dir).await;

    if let Some(tp) = entry.thumb_path.clone() {
        let thumb_path = Path::new(&tp);
        if thumb_path.exists() {
            // Serve existing thumbnail file directly.
            let file = File::open(thumb_path)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            let stream = ReaderStream::new(file);
            let body = StreamBody::new(stream);
            let boxed = axum::body::boxed(body);
            let mut res = Response::new(boxed);
            res.headers_mut()
                .insert("content-type", HeaderValue::from_static("image/jpeg"));
            return Ok(res);
        }
    }

    // If thumbnail missing, redirect to generator endpoint (async generation)
    if let Some(mt) = entry.mime_type.clone() {
        if mt.starts_with("image/") || mt.starts_with("video/") {
            let w = q.w.unwrap_or(200);
            let h = q.h.unwrap_or(200);
            let url = if let Some(id) = q.id {
                format!("/media/generate_thumbnail?id={}&w={}&h={}", id, w, h)
            } else {
                format!(
                    "/media/generate_thumbnail?path={}&w={}&h={}",
                    entry.path, w, h
                )
            };
            return Ok(Redirect::temporary(url.as_str()).into_response());
        }
    }

    Err((
        StatusCode::BAD_REQUEST,
        "No thumbnail available".to_string(),
    ))
}

pub async fn generate_thumbnail_handler(
    state: State<Arc<Mutex<AppState>>>,
    axum::extract::Query(q): axum::extract::Query<GenThumbQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let guard = state.0.lock().await;
    let pool = guard.pool.clone();
    let media_root = guard.directory_to_scan.clone();
    let ffmpeg_enabled = guard.ffmpeg_enabled;
    let ffmpeg_path = guard
        .ffmpeg_path
        .clone()
        .unwrap_or_else(|| "ffmpeg".to_string());
    let ffprobe_path = guard
        .ffprobe_path
        .clone()
        .unwrap_or_else(|| "ffprobe".to_string());
    drop(guard);

    // locate entry by id or path
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

    let w = q.w.unwrap_or(200);
    let h = q.h.unwrap_or(200);
    // use configured thumbnails dir (should be set by main)
    let thumbs_dir = PathBuf::from(
        state
            .0
            .lock()
            .await
            .thumbnails_dir
            .clone()
            .expect("thumbnails_dir must be configured in AppState"),
    );
    let _ = tokio::fs::create_dir_all(&thumbs_dir).await;
    let out_name = format!("{}_{}x{}.jpg", entry.id, w, h);
    let out_path = thumbs_dir.join(&out_name);
    // atomic temp path (keep .jpg extension so image crate can detect format)
    let tmp_name = format!(
        "{}.{}.jpg",
        out_name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let tmp_path = thumbs_dir.join(&tmp_name);

    if !out_path.exists() {
        let src = Path::new(&media_root).join(&entry.path);

        // Image handling
        if entry
            .mime_type
            .as_deref()
            .unwrap_or("")
            .starts_with("image/")
        {
            let img = image::open(&src)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            let thumb = img.thumbnail(w, h);
            // save to tmp then rename for atomicity
            thumb
                .save(&tmp_path)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            std::fs::rename(&tmp_path, &out_path)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            // best-effort DB update to set thumb_path/size
            let ne = models::NewMediaEntry {
                name: entry.name.clone(),
                path: entry.path.clone(),
                parent_id: entry.parent_id,
                mime_type: entry.mime_type.clone(),
                size: entry.size,
                tags: entry.tags.clone(),
                thumb_path: Some(out_path.to_string_lossy().to_string()),
                width: Some(thumb.width() as i64),
                height: Some(thumb.height() as i64),
                duration_secs: entry.duration_secs,
            };
            let _ = db::upsert_media(pool.clone(), &ne).await;
        }
        // Video handling via ffmpeg (best-effort)
        else if entry
            .mime_type
            .as_deref()
            .unwrap_or("")
            .starts_with("video/")
        {
            // Try to probe duration with ffprobe (optional)
            let mut duration_secs_opt: Option<i64> = None;
            if !ffmpeg_enabled {
                // ffmpeg integration is disabled; return non-fatal redirect to the expected static path
                let url = format!("/thumbnails/{}", out_name);
                return Ok(Redirect::temporary(url.as_str()));
            }

            if let Ok(output) = Command::new(ffprobe_path.as_str())
                .args([
                    "-v",
                    "error",
                    "-show_entries",
                    "format=duration",
                    "-of",
                    "default=noprint_wrappers=1:nokey=1",
                    src.to_string_lossy().as_ref(),
                ])
                .output()
                .await
            {
                if output.status.success() {
                    if let Ok(s) = String::from_utf8(output.stdout) {
                        if let Ok(f) = s.trim().parse::<f64>() {
                            duration_secs_opt = Some(f.round() as i64);
                        }
                    }
                }
            }

            // Choose a seek time: prefer 1s or 10% into duration if available
            let seek_time = if let Some(d) = duration_secs_opt {
                let ten_percent = (d as f64 * 0.1).round() as i64;
                let chosen = if d > 3 { 1 } else { ten_percent.max(0) };
                chosen
            } else {
                1
            };

            // Run ffmpeg to extract a frame
            let out_path_str = tmp_path.to_string_lossy().to_string();
            // Use -ss before -i for fast seek, and scale width preserving aspect ratio
            let ffmpeg_result = Command::new(ffmpeg_path.as_str())
                .args([
                    "-ss",
                    &format!("{}", seek_time),
                    "-i",
                    src.to_string_lossy().as_ref(),
                    "-frames:v",
                    "1",
                    "-q:v",
                    "2",
                    "-vf",
                    &format!("scale={}:-1", w),
                    "-y",
                    out_path_str.as_str(),
                ])
                .output()
                .await;

            match ffmpeg_result {
                Ok(out) => {
                    if !out.status.success() {
                        // ffmpeg failed; log stderr and fall back to placeholder thumbnail
                        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                        tracing::error!("ffmpeg failed for {}: {}", entry.path, stderr);
                        let url = "/thumbnails/placeholder.jpg";
                        return Ok(Redirect::temporary(url));
                    }
                    // Try to read generated image from tmp and then move into place atomically
                    if let Ok(img) = image::open(&tmp_path) {
                        let thumb = img.thumbnail(w, h);
                        // overwrite tmp with a nicely resized thumb
                        let _ = thumb.save(&tmp_path);
                        std::fs::rename(&tmp_path, &out_path)
                            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                        let ne = models::NewMediaEntry {
                            name: entry.name.clone(),
                            path: entry.path.clone(),
                            parent_id: entry.parent_id,
                            mime_type: entry.mime_type.clone(),
                            size: entry.size,
                            tags: entry.tags.clone(),
                            thumb_path: Some(out_path.to_string_lossy().to_string()),
                            width: Some(thumb.width() as i64),
                            height: Some(thumb.height() as i64),
                            duration_secs: duration_secs_opt.or(entry.duration_secs),
                        };
                        let _ = db::upsert_media(pool.clone(), &ne).await;
                    }
                }
                Err(_e) => {
                    // failed to spawn ffmpeg; fall back to placeholder thumbnail
                    let url = "/thumbnails/placeholder.jpg";
                    return Ok(Redirect::temporary(url));
                }
            }
        } else {
            return Err((
                StatusCode::BAD_REQUEST,
                "Unsupported media type".to_string(),
            ));
        }
    }

    // redirect to static file
    let url = format!("/thumbnails/{}", out_name);
    Ok(Redirect::temporary(url.as_str()))
}
