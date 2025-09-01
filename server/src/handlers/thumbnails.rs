use crate::db;
use crate::models;
use crate::state::AppState;
use axum::body::StreamBody;
use axum::http::HeaderValue;
use axum::response::{IntoResponse, Redirect, Response};
use axum::{extract::State, http::StatusCode};
use httpdate::fmt_http_date;
use image::{imageops::FilterType, ImageOutputFormat};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;
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
        // We store thumbnail paths as URL paths under `/thumbnails/<name>`.
        // Resolve the final segment and serve the corresponding file from
        // the configured thumbnails directory.
        if let Some(fname) = Path::new(&tp).file_name().and_then(|s| s.to_str()) {
            let fs_path = thumbs_dir.join(fname);
            if fs_path.exists() {
                let meta = tokio::fs::metadata(&fs_path)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                let total_size = meta.len();
                let modified = meta.modified().ok();

                // compute etag
                let mut hasher = Sha256::new();
                hasher.update(entry.path.as_bytes());
                hasher.update(&total_size.to_le_bytes());
                if let Some(m) = modified {
                    if let Ok(dur) = m.duration_since(UNIX_EPOCH) {
                        hasher.update(&dur.as_secs().to_le_bytes());
                        hasher.update(&dur.subsec_nanos().to_le_bytes());
                    }
                }
                let res = hasher.finalize();
                let etag = format!("\"{:x}\"", res);

                // We set ETag and Last-Modified headers; conditional GETs are mostly handled by the static /thumbnails mount.

                let file = File::open(&fs_path)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                let stream = ReaderStream::new(file);
                let body = StreamBody::new(stream);
                let boxed = axum::body::boxed(body);
                let mut res = Response::new(boxed);
                res.headers_mut()
                    .insert("content-type", HeaderValue::from_static("image/jpeg"));
                res.headers_mut()
                    .insert("ETag", HeaderValue::from_str(&etag).unwrap());
                if let Some(m) = modified {
                    let s = fmt_http_date(m);
                    res.headers_mut().insert(
                        "Last-Modified",
                        HeaderValue::from_str(&s).unwrap_or(HeaderValue::from_static("")),
                    );
                }
                return Ok(res);
            }
        }
    }

    // If thumbnail missing, redirect to generator endpoint (async generation)
    if let Some(mt) = entry.mime_type.clone() {
        if mt.starts_with("image/") || mt.starts_with("video/") {
            let w = q.w.unwrap_or(500);
            let h = q.h.unwrap_or(500);
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
    // Delegate to the shared generator helper
    // locate entry by id or path
    let guard = state.0.lock().await;
    let pool = guard.pool.clone();
    drop(guard);

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

    let w = q.w.unwrap_or(500);
    let h = q.h.unwrap_or(500);

    match generate_thumbnail_for_entry(state.0.clone(), &entry, w, h).await {
        Ok(out_name) => {
            let url = format!("/thumbnails/{}", out_name);
            Ok(Redirect::temporary(url.as_str()))
        }
        Err(e) => {
            // generation failed; return placeholder redirect
            tracing::error!("thumbnail generation failed: {}", e);
            let url = "/thumbnails/placeholder.jpg";
            Ok(Redirect::temporary(url))
        }
    }
}

/// Generate thumbnail for a specific media entry. Returns the output filename on success.
pub async fn generate_thumbnail_for_entry(
    state: Arc<Mutex<AppState>>,
    entry: &models::MediaEntry,
    w: u32,
    h: u32,
) -> Result<String, String> {
    // Acquire relevant config from state
    let guard = state.lock().await;
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
    let thumbs_dir = PathBuf::from(
        guard
            .thumbnails_dir
            .clone()
            .expect("thumbnails_dir must be configured in AppState"),
    );
    drop(guard);

    let _ = tokio::fs::create_dir_all(&thumbs_dir)
        .await
        .map_err(|e| e.to_string())?;

    let out_name = format!("{}_{}x{}.jpg", entry.id, w, h);
    let out_path = thumbs_dir.join(&out_name);
    if out_path.exists() {
        return Ok(out_name);
    }

    // atomic temp path
    let tmp_name = format!(
        "{}.{}.jpg",
        out_name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let tmp_path = thumbs_dir.join(&tmp_name);

    let src = Path::new(&media_root).join(&entry.path);

    if entry
        .mime_type
        .as_deref()
        .unwrap_or("")
        .starts_with("image/")
    {
        let img = image::open(&src).map_err(|e| e.to_string())?;
        let thumb = img.resize(w, h, FilterType::Lanczos3);
        let mut out_file = std::fs::File::create(&tmp_path).map_err(|e| e.to_string())?;
        thumb
            .write_to(
                &mut std::io::BufWriter::new(&mut out_file),
                ImageOutputFormat::Jpeg(88),
            )
            .map_err(|e| e.to_string())?;
        std::fs::rename(&tmp_path, &out_path).map_err(|e| e.to_string())?;
        let ne = models::NewMediaEntry {
            name: entry.name.clone(),
            path: entry.path.clone(),
            parent_id: entry.parent_id,
            mime_type: entry.mime_type.clone(),
            size: entry.size,
            tags: entry.tags.clone(),
            thumb_path: Some(format!("/thumbnails/{}", out_name)),
            width: Some(thumb.width() as i64),
            height: Some(thumb.height() as i64),
            duration_secs: entry.duration_secs,
        };
        let _ = db::upsert_media(pool.clone(), &ne).await;
        return Ok(out_name);
    } else if entry
        .mime_type
        .as_deref()
        .unwrap_or("")
        .starts_with("video/")
    {
        if !ffmpeg_enabled {
            return Err("ffmpeg disabled".to_string());
        }
        // probe duration
        let mut duration_secs_opt: Option<i64> = None;
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
        let seek_time = if let Some(d) = duration_secs_opt {
            let ten = (d as f64 * 0.1).round() as i64;
            if d > 3 {
                1
            } else {
                ten.max(0)
            }
        } else {
            1
        };
        let out_path_str = tmp_path.to_string_lossy().to_string();
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
                    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                    tracing::error!("ffmpeg failed for {}: {}", entry.path, stderr);
                    return Err("ffmpeg failed".to_string());
                }
                if let Ok(img) = image::open(&tmp_path) {
                    let tw = img.width();
                    let th = img.height();
                    std::fs::rename(&tmp_path, &out_path).map_err(|e| e.to_string())?;
                    let ne = models::NewMediaEntry {
                        name: entry.name.clone(),
                        path: entry.path.clone(),
                        parent_id: entry.parent_id,
                        mime_type: entry.mime_type.clone(),
                        size: entry.size,
                        tags: entry.tags.clone(),
                        thumb_path: Some(format!("/thumbnails/{}", out_name)),
                        width: Some(tw as i64),
                        height: Some(th as i64),
                        duration_secs: duration_secs_opt.or(entry.duration_secs),
                    };
                    let _ = db::upsert_media(pool.clone(), &ne).await;
                    return Ok(out_name);
                }
            }
            Err(_e) => {
                return Err("failed to spawn ffmpeg".to_string());
            }
        }
    }

    Err("unsupported media type or generation failed".to_string())
}
