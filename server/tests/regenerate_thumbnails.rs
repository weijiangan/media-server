use axum::extract::State;
use image::{ImageBuffer, Rgb};
use server::config::AppConfig;
use server::db;
use server::handlers::admin::regenerate_thumbnails_handler;
use server::models::NewMediaEntry;
use server::state::AppState;
use sqlx::sqlite::SqlitePoolOptions;
use std::collections::HashMap as StdHashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex as TokioMutex, Semaphore};

#[tokio::test]
async fn concurrent_regenerate() {
    // Setup repo-local temp directories under <crate>/tests/tmp
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let base = crate_root.join("tests").join("tmp").join(format!(
        "media_server_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let media_dir = base.join("media");
    let thumbs_dir = base.join("thumbnails");
    let _ = std::fs::create_dir_all(&media_dir);
    let _ = std::fs::create_dir_all(&thumbs_dir);

    // Create a tiny test image
    let img_path = media_dir.join("test.jpg");
    let img = ImageBuffer::from_pixel(64, 64, Rgb([120u8, 160u8, 200u8]));
    img.save(&img_path).unwrap();

    // Build a config with a file-based sqlite DB in the temp dir
    let db_path = base.join("media.db");
    let cfg = AppConfig {
        db_path: db_path.to_string_lossy().to_string(),
        directory_to_scan: media_dir.to_string_lossy().to_string(),
        host: None,
        port: None,
        ffmpeg_enabled: Some(false),
        ffmpeg_path: None,
        ffprobe_path: None,
        thumbnails_dir: Some(thumbs_dir.to_string_lossy().to_string()),
        cors_allowed_origins: None,
        cors_allow_credentials: None,
    };

    // init DB (use in-memory SQLite to avoid file permission issues)
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite::memory:")
        .await
        .expect("create in-memory db");
    db::initialize_database(pool.clone())
        .await
        .expect("init db");

    // Insert media entry pointing to our test image
    let ne = NewMediaEntry {
        name: "test".to_string(),
        path: "test.jpg".to_string(),
        parent_id: None,
        mime_type: Some("image/jpeg".to_string()),
        size: Some(0),
        tags: None,
        thumb_path: None,
        width: None,
        height: None,
        duration_secs: None,
    };
    let id = db::upsert_media(pool.clone(), &ne)
        .await
        .expect("upsert media");

    // Build AppState
    let state = AppState {
        pool: pool.clone(),
        directory_to_scan: cfg.directory_to_scan.clone(),
        ffmpeg_enabled: false,
        ffmpeg_path: None,
        ffprobe_path: None,
        thumbnails_dir: Some(thumbs_dir.to_string_lossy().to_string()),
        regen_semaphore: Arc::new(Semaphore::new(4)),
        in_flight: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
    };

    let state_arc = Arc::new(TokioMutex::new(state));

    // Prepare query params
    let mut params = StdHashMap::new();
    params.insert("w".to_string(), "100".to_string());
    params.insert("h".to_string(), "100".to_string());
    params.insert("concurrency".to_string(), "2".to_string());

    // Call the handler concurrently twice
    let s1 = state_arc.clone();
    let p1 = params.clone();
    let h1 = tokio::spawn(async move {
        regenerate_thumbnails_handler(State(s1), axum::extract::Query(p1)).await
    });

    let s2 = state_arc.clone();
    let p2 = params.clone();
    let h2 = tokio::spawn(async move {
        regenerate_thumbnails_handler(State(s2), axum::extract::Query(p2)).await
    });

    let r1 = h1.await.unwrap();
    let r2 = h2.await.unwrap();

    assert!(r1.is_ok(), "first request failed: {:?}", r1.err());
    assert!(r2.is_ok(), "second request failed: {:?}", r2.err());

    // ensure thumbnail exists
    let out_name = format!("{}_{}x{}.jpg", id, 100, 100);
    let out_path = thumbs_dir.join(&out_name);
    assert!(
        out_path.exists(),
        "thumbnail not created: {}",
        out_path.display()
    );

    // cleanup
    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn missing_file_regenerate() {
    // Setup repo-local temp directories under <crate>/tests/tmp
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let base = crate_root.join("tests").join("tmp").join(format!(
        "media_server_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let media_dir = base.join("media");
    let thumbs_dir = base.join("thumbnails");
    let _ = std::fs::create_dir_all(&media_dir);
    let _ = std::fs::create_dir_all(&thumbs_dir);

    // Do NOT create the image file; this should cause generation to fail

    // Build a config with a file-based sqlite DB in the temp dir
    let db_path = base.join("media.db");
    let cfg = AppConfig {
        db_path: db_path.to_string_lossy().to_string(),
        directory_to_scan: media_dir.to_string_lossy().to_string(),
        host: None,
        port: None,
        ffmpeg_enabled: Some(false),
        ffmpeg_path: None,
        ffprobe_path: None,
        thumbnails_dir: Some(thumbs_dir.to_string_lossy().to_string()),
        cors_allowed_origins: None,
        cors_allow_credentials: None,
    };

    // init DB (use in-memory SQLite to avoid file permission issues)
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite::memory:")
        .await
        .expect("create in-memory db");
    db::initialize_database(pool.clone())
        .await
        .expect("init db");

    // Insert media entry pointing to a missing image
    let ne = NewMediaEntry {
        name: "missing".to_string(),
        path: "missing.jpg".to_string(),
        parent_id: None,
        mime_type: Some("image/jpeg".to_string()),
        size: Some(0),
        tags: None,
        thumb_path: None,
        width: None,
        height: None,
        duration_secs: None,
    };
    let _id = db::upsert_media(pool.clone(), &ne)
        .await
        .expect("upsert media");

    // Build AppState
    let state = AppState {
        pool: pool.clone(),
        directory_to_scan: cfg.directory_to_scan.clone(),
        ffmpeg_enabled: false,
        ffmpeg_path: None,
        ffprobe_path: None,
        thumbnails_dir: Some(thumbs_dir.to_string_lossy().to_string()),
        regen_semaphore: Arc::new(Semaphore::new(4)),
        in_flight: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
    };

    let state_arc = Arc::new(TokioMutex::new(state));

    // Prepare query params
    let mut params = StdHashMap::new();
    params.insert("w".to_string(), "100".to_string());
    params.insert("h".to_string(), "100".to_string());
    params.insert("concurrency".to_string(), "2".to_string());

    let res = regenerate_thumbnails_handler(State(state_arc), axum::extract::Query(params))
        .await
        .expect("handler failed");

    // Expect a failure count > 0
    assert!(
        res.0.failed > 0 || res.0.done == 0,
        "expected failures for missing file"
    );

    // cleanup
    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test]
async fn many_waiters_regenerate() {
    // Setup repo-local temp directories under <crate>/tests/tmp
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let base = crate_root.join("tests").join("tmp").join(format!(
        "media_server_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let media_dir = base.join("media");
    let thumbs_dir = base.join("thumbnails");
    let _ = std::fs::create_dir_all(&media_dir);
    let _ = std::fs::create_dir_all(&thumbs_dir);

    // Create a tiny test image
    let img_path = media_dir.join("test2.jpg");
    let img = ImageBuffer::from_pixel(64, 64, Rgb([10u8, 20u8, 30u8]));
    img.save(&img_path).unwrap();

    // Build a config with a file-based sqlite DB in the temp dir
    let db_path = base.join("media.db");
    let cfg = AppConfig {
        db_path: db_path.to_string_lossy().to_string(),
        directory_to_scan: media_dir.to_string_lossy().to_string(),
        host: None,
        port: None,
        ffmpeg_enabled: Some(false),
        ffmpeg_path: None,
        ffprobe_path: None,
        thumbnails_dir: Some(thumbs_dir.to_string_lossy().to_string()),
        cors_allowed_origins: None,
        cors_allow_credentials: None,
    };

    // init DB (use in-memory SQLite to avoid file permission issues)
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite::memory:")
        .await
        .expect("create in-memory db");
    db::initialize_database(pool.clone())
        .await
        .expect("init db");

    // Insert media entry pointing to our test image
    let ne = NewMediaEntry {
        name: "test2".to_string(),
        path: "test2.jpg".to_string(),
        parent_id: None,
        mime_type: Some("image/jpeg".to_string()),
        size: Some(0),
        tags: None,
        thumb_path: None,
        width: None,
        height: None,
        duration_secs: None,
    };
    let id = db::upsert_media(pool.clone(), &ne)
        .await
        .expect("upsert media");

    // Build AppState
    let state = AppState {
        pool: pool.clone(),
        directory_to_scan: cfg.directory_to_scan.clone(),
        ffmpeg_enabled: false,
        ffmpeg_path: None,
        ffprobe_path: None,
        thumbnails_dir: Some(thumbs_dir.to_string_lossy().to_string()),
        regen_semaphore: Arc::new(Semaphore::new(4)),
        in_flight: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
    };

    let state_arc = Arc::new(TokioMutex::new(state));

    // Prepare query params
    let mut params = StdHashMap::new();
    params.insert("w".to_string(), "80".to_string());
    params.insert("h".to_string(), "80".to_string());
    params.insert("concurrency".to_string(), "4".to_string());

    // Spawn many concurrent callers (simulate multiple clients)
    let mut handles = Vec::new();
    for _ in 0..8 {
        let s = state_arc.clone();
        let p = params.clone();
        handles.push(tokio::spawn(async move {
            regenerate_thumbnails_handler(State(s), axum::extract::Query(p)).await
        }));
    }

    let mut success = 0;
    for h in handles {
        let r = h.await.unwrap();
        if r.is_ok() {
            success += 1;
        }
    }

    assert_eq!(success, 8, "all callers should succeed");

    // ensure thumbnail exists and DB updated
    let out_name = format!("{}_{}x{}.jpg", id, 80, 80);
    let out_path = thumbs_dir.join(&out_name);
    assert!(
        out_path.exists(),
        "thumbnail not created: {}",
        out_path.display()
    );

    let entry = db::get_media_by_id(pool.clone(), id).await.expect("db get");
    assert!(entry.unwrap().thumb_path.is_some(), "DB thumb_path not set");

    // cleanup
    let _ = std::fs::remove_dir_all(&base);
}
