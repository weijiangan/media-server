mod config;
mod db;
mod handlers;
mod models;
mod scanner;
mod state;

use axum::http::{HeaderValue, Method};
use axum::routing::get_service;
use config::AppConfig;
use db::initialize_database;
use handlers::{
    generate_thumbnail_handler, get_file_details_handler, list_directory_handler, stream_handler,
    thumbnail_handler, trigger_scan_handler,
};
use state::AppState;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

use ::config::{builder::DefaultState, ConfigBuilder, File};
use axum::{
    routing::{get, post},
    Router,
};
use image::{ImageOutputFormat, RgbImage};
use sqlx::sqlite::SqlitePoolOptions;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing_subscriber;

use clap::Command as ClapApp;

fn main() {
    let matches = ClapApp::new("Media Server")
        .version("1.0")
        .author("Your Name")
        .about("Media server with directory scanning and REST API")
        .subcommand(ClapApp::new("scan").about("Trigger a directory scan"))
        .get_matches();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build runtime");

    rt.block_on(async {
        // initialize tracing subscriber (reads RUST_LOG env)
        tracing_subscriber::fmt::init();
        let settings = ConfigBuilder::<DefaultState>::default()
            .add_source(File::with_name("config"))
            .build()
            .expect("Failed to load configuration");
        let config: AppConfig = settings
            .try_deserialize()
            .expect("Invalid configuration format");

        let db_path = PathBuf::from(&config.db_path);
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .expect("Failed to create parent directory for the database file");
        }

        // log resolved DB path and create the file if missing
        tracing::info!("Resolved DB path: {}", db_path.display());
        if db_path.exists() {
            if db_path.is_dir() {
                panic!("Configured db_path is a directory: {}", db_path.display());
            }
        } else {
            std::fs::File::create(&db_path).expect("Failed to create database file");
            println!("Created empty database file at {}", db_path.display());
        }

        let db_url = format!("sqlite://{}", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
            .expect("Failed to create sqlx pool");

        initialize_database(pool.clone())
            .await
            .expect("db init failed");

        if let Some(_) = matches.subcommand_matches("scan") {
            println!("Starting directory scan...");
            if let Err(e) = scanner::scan_directory_and_index(
                pool.clone(),
                config.directory_to_scan.clone(),
                None,
            )
            .await
            {
                tracing::error!("Error scanning directory: {}", e);
            }
            println!("Directory scan completed.");
            return;
        }

        // resolve thumbnails directory (configurable)
        let thumbnails_dir_path = if let Some(t) = config.thumbnails_dir.clone() {
            std::path::PathBuf::from(t)
        } else {
            // Prefer system cache for root, per-user XDG otherwise
            if nix::unistd::Uid::effective().is_root() {
                std::path::PathBuf::from("/var/cache/media-server/thumbnails")
            } else if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
                std::path::PathBuf::from(xdg).join("media-server/thumbnails")
            } else if let Some(home) = dirs::home_dir() {
                home.join(".cache/media-server/thumbnails")
            } else {
                std::path::PathBuf::from(".thumbnails")
            }
        };

        let state = Arc::new(Mutex::new(AppState {
            pool: pool.clone(),
            directory_to_scan: config.directory_to_scan.clone(),
            ffmpeg_enabled: config.ffmpeg_enabled.unwrap_or(false),
            ffmpeg_path: config.ffmpeg_path.clone(),
            ffprobe_path: config.ffprobe_path.clone(),
            thumbnails_dir: Some(thumbnails_dir_path.to_string_lossy().to_string()),
        }));

        // ensure dir exists and serve thumbnails from resolved location
        let _ = std::fs::create_dir_all(&thumbnails_dir_path);
        tracing::info!("Thumbnails directory: {}", thumbnails_dir_path.display());

        // Ensure a placeholder thumbnail exists so redirects to
        // `/thumbnails/placeholder.jpg` never 404. We create a tiny 16x16
        // gray JPEG if missing.
        let placeholder_path = thumbnails_dir_path.join("placeholder.jpg");
        if !placeholder_path.exists() {
            if let Ok(mut buf) = std::fs::File::create(&placeholder_path) {
                // Create a 16x16 gray image
                let img = RgbImage::from_pixel(16, 16, image::Rgb([200u8, 200u8, 200u8]));
                // Encode JPEG into the file
                let _ = image::DynamicImage::ImageRgb8(img).write_to(
                    &mut std::io::BufWriter::new(&mut buf),
                    ImageOutputFormat::Jpeg(75),
                );
            }
        }

        // Garbage-collect stale temporary thumbnail files left from crashes or
        // interrupted runs. We match filenames with the pattern
        // `<id>_<wxh>.<nanos>.jpg` (i.e. have an extra `.` before the .jpg) and
        // remove ones older than 1 hour.
        if let Ok(entries) = std::fs::read_dir(&thumbnails_dir_path) {
            let now = std::time::SystemTime::now();
            for e in entries.flatten() {
                if let Ok(fname) = e.file_name().into_string() {
                    if fname.ends_with(".jpg") && fname.matches('.').count() >= 2 {
                        if let Ok(meta) = e.metadata() {
                            if let Ok(modified) = meta.modified() {
                                if let Ok(age) = now.duration_since(modified) {
                                    if age.as_secs() > 60 * 60 {
                                        let _ = std::fs::remove_file(e.path());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let serve_thumbs = get_service(ServeDir::new(thumbnails_dir_path)).handle_error(
            |e: std::io::Error| async move {
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Unhandled internal error: {}", e),
                )
            },
        );
        // (previously created serve_thumbs above)

        // Build a CorsLayer from configuration. If `cors_allowed_origins`
        // is set in config, use that whitelist; otherwise fall back to a
        // sensible default (127.0.0.1:8081).
        let mut cors_layer = CorsLayer::new()
            .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
            .allow_headers(Any);

        let allow_credentials = config.cors_allow_credentials.unwrap_or(false);
        if allow_credentials {
            cors_layer = cors_layer.allow_credentials(true);
        }

        if let Some(origins) = config.cors_allowed_origins.clone() {
            if origins.is_empty() {
                cors_layer = cors_layer.allow_origin(Any);
            } else if origins.len() == 1 {
                match HeaderValue::from_str(&origins[0]) {
                    Ok(hv) => {
                        cors_layer =
                            cors_layer.allow_origin(tower_http::cors::AllowOrigin::exact(hv))
                    }
                    Err(_) => cors_layer = cors_layer.allow_origin(Any),
                }
            } else {
                let list: Vec<HeaderValue> = origins
                    .into_iter()
                    .filter_map(|s| HeaderValue::from_str(&s).ok())
                    .collect();
                if !list.is_empty() {
                    cors_layer = cors_layer.allow_origin(tower_http::cors::AllowOrigin::list(list));
                } else {
                    cors_layer = cors_layer.allow_origin(Any);
                }
            }
        } else {
            // Default single allowed origin for local dev
            let origin = HeaderValue::from_static("http://127.0.0.1:8081");
            cors_layer = cors_layer.allow_origin(tower_http::cors::AllowOrigin::exact(origin));
        }

        let cors = cors_layer;

        let app = Router::new()
            .route("/scan", post(trigger_scan_handler))
            .route("/media", get(list_directory_handler))
            .route("/media/details", get(get_file_details_handler))
            .route("/media/thumbnail", get(thumbnail_handler))
            .route("/media/generate_thumbnail", get(generate_thumbnail_handler))
            .route("/media/stream", get(stream_handler))
            .route("/media/image", get(stream_handler))
            .nest_service("/thumbnails", serve_thumbs)
            .with_state(state.clone())
            .layer(cors);

        let host = config.host.unwrap_or_else(|| "127.0.0.1".to_string());
        let port = config.port.unwrap_or(8080);
        let bind_addr = format!("{}:{}", host, port);

        axum::Server::bind(&bind_addr.parse().expect("Invalid bind address"))
            .serve(app.into_make_service())
            .await
            .unwrap();
    });
}
