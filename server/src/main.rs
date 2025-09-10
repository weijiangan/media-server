use axum::{
    routing::{get, post},
    Router,
};
use server::handlers::admin;
use server::handlers::{
    generate_thumbnail_handler, get_file_details_handler, list_directory_handler, stream_handler,
    thumbnail_handler, trigger_scan_handler,
};
use server::state::AppState;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use tracing_subscriber;

use clap::{Arg, Command as ClapApp};

use server::startup::{
    build_client_service, build_cors, build_thumbnails_service, init_db, load_config,
    prepare_thumbnails_cache, resolve_client_dist_dir, resolve_thumbnails_dir,
};

fn main() {
    let matches = ClapApp::new("Media Server")
        .version("1.0")
        .author("Your Name")
        .about("Media server with directory scanning and REST API")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE.json")
                .help("Path to config JSON file (overrides search)")
                .num_args(1),
        )
        .subcommand(ClapApp::new("scan").about("Trigger a directory scan"))
        .get_matches();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build runtime");

    rt.block_on(async {
        // initialize tracing subscriber (reads RUST_LOG env)
        tracing_subscriber::fmt::init();
        let config = match load_config(matches.get_one::<String>("config").map(|s| s.into())) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error loading configuration: {}", e);
                std::process::exit(1);
            }
        };

        let pool = init_db(&config).await;

        if let Some(_) = matches.subcommand_matches("scan") {
            println!("Starting directory scan...");
            if let Err(e) = server::scanner::scan_directory_and_index(
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
        let thumbnails_dir_path = resolve_thumbnails_dir(&config);

        let state = Arc::new(Mutex::new(AppState {
            pool: pool.clone(),
            directory_to_scan: config.directory_to_scan.clone(),
            ffmpeg_enabled: config.ffmpeg_enabled.unwrap_or(false),
            ffmpeg_path: config.ffmpeg_path.clone(),
            ffprobe_path: config.ffprobe_path.clone(),
            thumbnails_dir: Some(thumbnails_dir_path.to_string_lossy().to_string()),
            client_dist_dir: config.client_dist_dir.clone(),
            // regeneration controls
            regen_semaphore: Arc::new(Semaphore::new(4)),
            in_flight: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }));

        // ensure cache and build static service for thumbnails
        prepare_thumbnails_cache(&thumbnails_dir_path);
        let serve_thumbs = build_thumbnails_service(thumbnails_dir_path.clone());
        // (previously created serve_thumbs above)

        let cors_opt = match build_cors(&config) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("CORS configuration error: {}", e);
                std::process::exit(2);
            }
        };

        let mut app = Router::new()
            .route("/scan", post(trigger_scan_handler))
            .route("/media", get(list_directory_handler))
            .route("/media/details", get(get_file_details_handler))
            .route("/media/thumbnail", get(thumbnail_handler))
            .route("/media/generate_thumbnail", get(generate_thumbnail_handler))
            .route(
                "/admin/regenerate_thumbnails",
                post(admin::regenerate_thumbnails_handler),
            )
            .route("/media/stream", get(stream_handler))
            .route("/media/image", get(stream_handler))
            .nest_service("/thumbnails", serve_thumbs)
            .with_state(state.clone());
        // If client dist is configured, mount it as a fallback SPA service
        if let Some(cd) = resolve_client_dist_dir(&config) {
            let client_router = build_client_service(cd);
            app = app.merge(client_router);
        }
        if let Some(cors_layer) = cors_opt {
            app = app.layer(cors_layer);
        }

        // Log a concise startup summary
        server::startup::log_startup_info(&config);

        let host = config.host.unwrap_or_else(|| "127.0.0.1".to_string());
        let port = config.port.unwrap_or(8080);
        let bind_addr = format!("{}:{}", host, port);

        axum::Server::bind(&bind_addr.parse().expect("Invalid bind address"))
            .serve(app.into_make_service())
            .await
            .unwrap();
    });
}
