use crate::models::{MediaEntry, NewMediaEntry};
use serde_json;
use sqlx::{query, query_scalar, Sqlite, SqlitePool, Transaction};

pub async fn initialize_database(pool: SqlitePool) -> Result<(), sqlx::Error> {
    let create = r#"
        CREATE TABLE IF NOT EXISTS media (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            path TEXT NOT NULL UNIQUE,
            parent_id INTEGER,
            mime_type TEXT,
            size INTEGER,
            tags TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (parent_id) REFERENCES media (id)
        )
    "#;

    let idx_parent = "CREATE INDEX IF NOT EXISTS idx_parent_id ON media (parent_id)";
    let idx_path = "CREATE INDEX IF NOT EXISTS idx_path ON media (path)";

    query(create).execute(&pool).await?;
    query(idx_parent).execute(&pool).await?;
    query(idx_path).execute(&pool).await?;

    Ok(())
}

pub async fn upsert_media(pool: SqlitePool, entry: &NewMediaEntry) -> Result<i64, sqlx::Error> {
    let tags_json: Option<String> = entry
        .tags
        .as_ref()
        .and_then(|t| serde_json::to_string(t).ok());
    let q = r#"
        INSERT INTO media (name, path, parent_id, mime_type, size, tags)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(path) DO UPDATE SET
            name=excluded.name,
            parent_id=excluded.parent_id,
            mime_type=excluded.mime_type,
            size=excluded.size,
            tags=excluded.tags
    "#;

    query(q)
        .bind(&entry.name)
        .bind(&entry.path)
        .bind(entry.parent_id)
        .bind(&entry.mime_type)
        .bind(entry.size)
        .bind(&tags_json)
        .execute(&pool)
        .await?;

    let id: i64 = query_scalar("SELECT id FROM media WHERE path = ?1")
        .bind(&entry.path)
        .fetch_one(&pool)
        .await?;

    Ok(id)
}

pub async fn upsert_media_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    entry: &NewMediaEntry,
) -> Result<i64, sqlx::Error> {
    let tags_json: Option<String> = entry
        .tags
        .as_ref()
        .and_then(|t| serde_json::to_string(t).ok());
    let q = r#"
        INSERT INTO media (name, path, parent_id, mime_type, size, tags)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(path) DO UPDATE SET
            name=excluded.name,
            parent_id=excluded.parent_id,
            mime_type=excluded.mime_type,
            size=excluded.size,
            tags=excluded.tags
    "#;

    query(q)
        .bind(&entry.name)
        .bind(&entry.path)
        .bind(entry.parent_id)
        .bind(&entry.mime_type)
        .bind(entry.size)
        .bind(&tags_json)
        .execute(&mut **tx)
        .await?;

    let id: i64 = query_scalar("SELECT id FROM media WHERE path = ?1")
        .bind(&entry.path)
        .fetch_one(&mut **tx)
        .await?;

    Ok(id)
}

pub async fn get_media_by_id(pool: SqlitePool, id: i64) -> Result<Option<MediaEntry>, sqlx::Error> {
    let row = sqlx::query_as::<_, (i64, String, String, Option<i64>, Option<String>, Option<i64>, Option<String>, String)>(
        "SELECT id, name, path, parent_id, mime_type, size, tags, created_at FROM media WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(&pool)
    .await?;

    if let Some(r) = row {
        let tags: Option<Vec<String>> = r.6.as_ref().and_then(|s| serde_json::from_str(s).ok());
        Ok(Some(MediaEntry {
            id: r.0,
            name: r.1,
            path: r.2,
            parent_id: r.3,
            mime_type: r.4,
            size: r.5,
            created_at: r.7,
            tags,
        }))
    } else {
        Ok(None)
    }
}

pub async fn get_media_by_path(
    pool: SqlitePool,
    path: String,
) -> Result<Option<MediaEntry>, sqlx::Error> {
    let row = sqlx::query_as::<_, (i64, String, String, Option<i64>, Option<String>, Option<i64>, Option<String>, String)>(
        "SELECT id, name, path, parent_id, mime_type, size, tags, created_at FROM media WHERE path = ?1",
    )
    .bind(path)
    .fetch_optional(&pool)
    .await?;

    if let Some(r) = row {
        let tags: Option<Vec<String>> = r.6.as_ref().and_then(|s| serde_json::from_str(s).ok());
        Ok(Some(MediaEntry {
            id: r.0,
            name: r.1,
            path: r.2,
            parent_id: r.3,
            mime_type: r.4,
            size: r.5,
            created_at: r.7,
            tags,
        }))
    } else {
        Ok(None)
    }
}

pub async fn list_children(
    pool: SqlitePool,
    parent_id: Option<i64>,
    tags: Option<Vec<String>>,
) -> Result<Vec<MediaEntry>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (i64, String, String, Option<i64>, Option<String>, Option<i64>, Option<String>, String)>(
        "SELECT id, name, path, parent_id, mime_type, size, tags, created_at FROM media WHERE parent_id IS ?1",
    )
    .bind(parent_id)
    .fetch_all(&pool)
    .await?;

    let mut out = Vec::new();
    for r in rows {
        let tags_vec: Option<Vec<String>> = r.6.as_ref().and_then(|s| serde_json::from_str(s).ok());
        out.push(MediaEntry {
            id: r.0,
            name: r.1,
            path: r.2,
            parent_id: r.3,
            mime_type: r.4,
            size: r.5,
            created_at: r.7,
            tags: tags_vec,
        });
    }

    if let Some(filter_tags) = tags {
        out = out
            .into_iter()
            .filter(|entry| {
                if let Some(tlist) = &entry.tags {
                    filter_tags.iter().all(|ft| tlist.contains(ft))
                } else {
                    false
                }
            })
            .collect();
    }

    Ok(out)
}
