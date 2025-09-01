use crate::db;
use crate::handlers::thumbnails::generate_thumbnail_for_entry;
use crate::state::AppState;
use axum::{extract::State, response::Json};
use futures::stream::{self, StreamExt};
use serde::Serialize;
use sqlx::Row;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex as TokioMutex};

#[derive(Serialize, Debug)]
pub struct Progress {
    pub total: usize,
    pub done: usize,
    pub failed: usize,
}

// POST /admin/regenerate_thumbnails?w=200&h=200&concurrency=4
pub async fn regenerate_thumbnails_handler(
    State(state): State<Arc<TokioMutex<AppState>>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Progress>, (axum::http::StatusCode, String)> {
    let w = params
        .get("w")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(500);
    let h = params
        .get("h")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(500);
    let concurrency = params
        .get("concurrency")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(2);

    // snapshot state (capture shared controls too)
    let guard = state.lock().await;
    let pool = guard.pool.clone();
    let _thumbs_dir = guard.thumbnails_dir.clone();
    let _ffmpeg_enabled = guard.ffmpeg_enabled;
    let regen_sem = guard.regen_semaphore.clone();
    let in_flight = guard.in_flight.clone();
    drop(guard);

    // Query DB for entries that are images or videos
    let rows = sqlx::query("SELECT id, path, thumb_path FROM media WHERE mime_type LIKE 'image/%' OR mime_type LIKE 'video/%';")
        .fetch_all(&pool)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total = rows.len();
    let counter = Arc::new(tokio::sync::Mutex::new((0usize, 0usize))); // done, failed

    // we'll use the app-wide semaphore (regen_sem) to bound global concurrency.
    // the `concurrency` param still controls how many tasks we spawn concurrently.

    // Build a stream of rows and process with buffer_unordered to limit per-request concurrency
    let pool2 = pool.clone();
    let state2 = state.clone();
    let regen_sem2 = regen_sem.clone();
    let in_flight2 = in_flight.clone();
    let counter2 = counter.clone();

    // Build a stream of futures (one per DB row) and process them with buffer_unordered
    let tasks = rows.into_iter().map(|r| {
        let pool = pool2.clone();
        let state = state2.clone();
        let regen_sem = regen_sem2.clone();
        let in_flight = in_flight2.clone();
        let counter = counter2.clone();

        async move {
            let id: i64 = r.get("id");
            let _path: String = r.get("path");
            let _thumb_path: Option<String> = r.get("thumb_path");

            let key = format!("{}:{}x{}", id, w, h);

            // Create a oneshot channel to wait for completion if needed
            let (tx, rx) = oneshot::channel::<Result<(), String>>();

            // Register as a waiter if a job is already in-flight
            let mut should_generate = false;
            {
                let mut infl = in_flight.lock().await;
                if let Some(waiters) = infl.get_mut(&key) {
                    // another task is generating; register to wait
                    waiters.push(tx);
                } else {
                    // no in-flight job; insert a new waiters vec with our sender
                    infl.insert(key.clone(), vec![tx]);
                    should_generate = true;
                }
            }

            if !should_generate {
                // Wait for existing generation to complete
                match rx.await {
                    Ok(Ok(_)) => {
                        let mut c = counter.lock().await;
                        c.0 += 1;
                        return;
                    }
                    Ok(Err(_)) | Err(_) => {
                        let mut c = counter.lock().await;
                        c.1 += 1;
                        return;
                    }
                }
            }

            // Acquire global permit before performing the heavy work
            let _permit = regen_sem.acquire().await.expect("semaphore closed");

            // Re-fetch entry
            let res = match db::get_media_by_id(pool.clone(), id).await {
                Ok(Some(entry)) => {
                    match generate_thumbnail_for_entry(state.clone(), &entry, w, h).await {
                        Ok(_) => Ok(()),
                        Err(e) => Err(e),
                    }
                }
                Ok(None) => Err("not found".to_string()),
                Err(e) => Err(e.to_string()),
            };

            // notify all waiters and remove key
            let waiters = {
                let mut infl = in_flight.lock().await;
                infl.remove(&key)
            };
            if let Some(waiters) = waiters {
                for wtx in waiters {
                    let _ = wtx.send(res.clone());
                }
            }

            // record counters
            match res {
                Ok(_) => {
                    let mut c = counter.lock().await;
                    c.0 += 1;
                }
                Err(_) => {
                    let mut c = counter.lock().await;
                    c.1 += 1;
                }
            }
        }
    });

    // Drive the stream with bounded concurrency
    stream::iter(tasks)
        .buffer_unordered(concurrency)
        .for_each(|_| async {})
        .await;

    let (done, failed) = {
        let c = counter.lock().await;
        (c.0, c.1)
    };

    Ok(Json(Progress {
        total,
        done,
        failed,
    }))
}
