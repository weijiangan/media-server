use crate::{config::AppConfig, db::initialize_database};
use axum::http::{HeaderValue, Method};
use axum::routing::{get_service, MethodRouter};
use image::{ImageOutputFormat, RgbImage};
use sqlx::sqlite::SqlitePoolOptions;
use std::path::{Path, PathBuf};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

pub fn load_config(cli_path: Option<PathBuf>) -> Result<AppConfig, Box<dyn std::error::Error>> {
    use ::config::{builder::DefaultState, ConfigBuilder, File};

    let mut builder = ConfigBuilder::<DefaultState>::default();
    let mut chosen: Option<PathBuf> = None;

    // If CLI path is provided, use it as-is; let deserialization fail if format is wrong.
    if let Some(p) = cli_path {
        chosen = Some(p);
    } else {
        // Strict search: only look for .json files in known locations
        let push_if_exists = |p: PathBuf| -> Option<PathBuf> {
            if p.exists() {
                Some(p)
            } else {
                None
            }
        };

        // Prefer ./config.json (monorepo server dir)
        if let Ok(cwd) = std::env::current_dir() {
            if let Some(found) = push_if_exists(cwd.join("config.json")) {
                chosen = Some(found);
            }
        }
        // server/config.json
        if chosen.is_none() {
            if let Some(found) = push_if_exists(PathBuf::from("server/config.json")) {
                chosen = Some(found);
            }
        }
        // XDG config.json
        if chosen.is_none() {
            if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
                let p = PathBuf::from(xdg).join("media-server/config.json");
                if let Some(found) = push_if_exists(p) {
                    chosen = Some(found);
                }
            }
            if chosen.is_none() {
                if let Some(home) = dirs::home_dir() {
                    let p = home.join(".config/media-server/config.json");
                    if let Some(found) = push_if_exists(p) {
                        chosen = Some(found);
                    }
                }
            }
        }
        // /etc/media-server/config.json
        if chosen.is_none() {
            if let Some(found) = push_if_exists(PathBuf::from("/etc/media-server/config.json")) {
                chosen = Some(found);
            }
        }
    }

    if let Some(cfg_path) = chosen {
        tracing::info!("Using configuration file: {}", cfg_path.display());
        builder = builder.add_source(File::from(cfg_path));
    } else {
        return Err(format!("No config.json found. Provide --config <file.json> or place config.json in ./, server/, XDG (~/.config/media-server/), or /etc/media-server/").into());
    }

    let settings = builder
        .build()
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    let cfg: AppConfig = settings
        .try_deserialize()
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    Ok(cfg)
}

pub async fn init_db(config: &AppConfig) -> sqlx::SqlitePool {
    let db_path = PathBuf::from(&config.db_path);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .expect("Failed to create parent directory for the database file");
    }
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
    pool
}

pub fn resolve_thumbnails_dir(config: &AppConfig) -> PathBuf {
    if let Some(t) = config.thumbnails_dir.clone() {
        PathBuf::from(t)
    } else {
        if nix::unistd::Uid::effective().is_root() {
            PathBuf::from("/var/cache/media-server/thumbnails")
        } else if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
            PathBuf::from(xdg).join("media-server/thumbnails")
        } else if let Some(home) = dirs::home_dir() {
            home.join(".cache/media-server/thumbnails")
        } else {
            PathBuf::from(".thumbnails")
        }
    }
}

pub fn prepare_thumbnails_cache(thumbnails_dir_path: &Path) {
    let _ = std::fs::create_dir_all(thumbnails_dir_path);
    tracing::info!("Thumbnails directory: {}", thumbnails_dir_path.display());
    // Ensure placeholder
    let placeholder_path = thumbnails_dir_path.join("placeholder.jpg");
    if !placeholder_path.exists() {
        if let Ok(mut buf) = std::fs::File::create(&placeholder_path) {
            let img = RgbImage::from_pixel(16, 16, image::Rgb([200u8, 200u8, 200u8]));
            let _ = image::DynamicImage::ImageRgb8(img).write_to(
                &mut std::io::BufWriter::new(&mut buf),
                ImageOutputFormat::Jpeg(75),
            );
        }
    }
    // GC stale temp-like files
    if let Ok(entries) = std::fs::read_dir(thumbnails_dir_path) {
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
}

pub fn build_thumbnails_service(thumbnails_dir_path: PathBuf) -> MethodRouter {
    get_service(ServeDir::new(thumbnails_dir_path)).handle_error(|e: std::io::Error| async move {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Unhandled internal error: {}", e),
        )
    })
}

pub fn build_cors(config: &AppConfig) -> CorsLayer {
    let mut cors_layer = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);

    if config.cors_allow_credentials.unwrap_or(false) {
        cors_layer = cors_layer.allow_credentials(true);
    }

    if let Some(origins) = config.cors_allowed_origins.clone() {
        if origins.is_empty() {
            cors_layer = cors_layer.allow_origin(Any);
        } else if origins.len() == 1 {
            match HeaderValue::from_str(&origins[0]) {
                Ok(hv) => {
                    cors_layer = cors_layer.allow_origin(tower_http::cors::AllowOrigin::exact(hv))
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
        let origin = HeaderValue::from_static("http://127.0.0.1:8081");
        cors_layer = cors_layer.allow_origin(tower_http::cors::AllowOrigin::exact(origin));
    }

    cors_layer
}
