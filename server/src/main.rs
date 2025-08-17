mod config;
mod db;
mod handlers;
mod models;
mod scanner;
mod state;

use config::AppConfig;
use db::initialize_database;
use handlers::{get_file_details_handler, list_directory_handler, trigger_scan_handler};
use state::AppState;

use ::config::{builder::DefaultState, ConfigBuilder, File};
use axum::{
    routing::{get, post},
    Router,
};
use sqlx::sqlite::SqlitePoolOptions;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

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
        println!("Resolved DB path: {}", db_path.display());
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
                eprintln!("Error scanning directory: {}", e);
            }
            println!("Directory scan completed.");
            return;
        }

        let state = Arc::new(Mutex::new(AppState {
            pool: pool.clone(),
            directory_to_scan: config.directory_to_scan.clone(),
        }));

        let app = Router::new()
            .route("/scan", post(trigger_scan_handler))
            .route("/media", get(list_directory_handler))
            .route("/media/details", get(get_file_details_handler))
            .with_state(state.clone());

        let host = config.host.unwrap_or_else(|| "127.0.0.1".to_string());
        let port = config.port.unwrap_or(8080);
        let bind_addr = format!("{}:{}", host, port);

        axum::Server::bind(&bind_addr.parse().expect("Invalid bind address"))
            .serve(app.into_make_service())
            .await
            .unwrap();
    });
}
