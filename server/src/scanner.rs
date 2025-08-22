use crate::db;
use crate::models::NewMediaEntry;
use sqlx::SqlitePool;
use std::path::PathBuf;

const BATCH_SIZE: usize = 500;

// Helper function to process a batch of files in a single transaction
async fn flush_file_buffer(
    pool: &SqlitePool,
    buffer: &mut Vec<NewMediaEntry>,
) -> Result<(), sqlx::Error> {
    if buffer.is_empty() {
        return Ok(());
    }

    // Begin a transaction
    let mut tx = pool.begin().await?;

    // Process all files in the buffer
    while let Some(entry) = buffer.pop() {
        db::upsert_media_in_tx(&mut tx, &entry).await?;
    }

    // Commit the transaction
    tx.commit().await?;

    Ok(())
}

pub async fn scan_directory_and_index(
    pool: SqlitePool,
    directory: String,
    parent_id: Option<i64>,
) -> Result<(), String> {
    let root = PathBuf::from(&directory);

    let mut stack: Vec<(PathBuf, Option<i64>)> = vec![(PathBuf::from(directory), parent_id)];

    // Buffer for file entries to be upserted in batches
    let mut file_buffer: Vec<NewMediaEntry> = Vec::with_capacity(BATCH_SIZE);

    while let Some((dir_path, parent)) = stack.pop() {
        let mut read_dir = match tokio::fs::read_dir(&dir_path).await {
            Ok(rd) => rd,
            Err(_) => continue,
        };

        loop {
            match read_dir.next_entry().await {
                Ok(opt) => {
                    if let Some(entry) = opt {
                        let path = entry.path();
                        let name = entry.file_name().to_string_lossy().to_string();

                        // compute relative path from root
                        let rel_path = match path.strip_prefix(&root) {
                            Ok(p) => p.to_string_lossy().to_string(),
                            Err(_) => path.to_string_lossy().to_string(),
                        };

                        // use async metadata
                        let meta = match tokio::fs::metadata(&path).await {
                            Ok(m) => m,
                            Err(_) => continue,
                        };

                        if meta.is_dir() {
                            // Directories must be upserted immediately because we need their ID for traversal
                            let n = NewMediaEntry {
                                name: name.clone(),
                                path: rel_path.clone(),
                                parent_id: parent,
                                mime_type: None,
                                size: None,
                                tags: None,
                                thumb_path: None,
                                width: None,
                                height: None,
                                duration_secs: None,
                            };

                            let new_parent_id = db::upsert_media(pool.clone(), &n)
                                .await
                                .map_err(|e| format!("db upsert error: {}", e))?;

                            stack.push((path, Some(new_parent_id)));
                        } else if meta.is_file() {
                            let size = meta.len() as i64;
                            let mime_type = mime_guess::from_path(&path)
                                .first_or_octet_stream()
                                .to_string();

                            let n = NewMediaEntry {
                                name: name.clone(),
                                path: rel_path.clone(),
                                parent_id: parent,
                                mime_type: Some(mime_type),
                                size: Some(size),
                                tags: None,
                                thumb_path: None,
                                width: None,
                                height: None,
                                duration_secs: None,
                            };

                            // Buffer file entries for batch processing
                            file_buffer.push(n);

                            // When buffer reaches BATCH_SIZE, process the batch in a transaction
                            if file_buffer.len() >= BATCH_SIZE {
                                flush_file_buffer(&pool, &mut file_buffer)
                                    .await
                                    .map_err(|e| format!("Failed to flush file buffer: {}", e))?;
                            }
                        }
                    } else {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    }

    // Flush any remaining files in the buffer
    if !file_buffer.is_empty() {
        flush_file_buffer(&pool, &mut file_buffer)
            .await
            .map_err(|e| format!("Failed to flush file buffer: {}", e))?;
    }

    Ok(())
}
