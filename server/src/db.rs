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
            thumb_path TEXT,
            width INTEGER,
            height INTEGER,
            duration_secs INTEGER,
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
        INSERT INTO media (name, path, parent_id, mime_type, size, tags, thumb_path, width, height, duration_secs)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ON CONFLICT(path) DO UPDATE SET
            name=excluded.name,
            parent_id=excluded.parent_id,
            mime_type=excluded.mime_type,
            size=excluded.size,
            tags=excluded.tags,
            thumb_path=excluded.thumb_path,
            width=excluded.width,
            height=excluded.height,
            duration_secs=excluded.duration_secs
    "#;

    query(q)
        .bind(&entry.name)
        .bind(&entry.path)
        .bind(entry.parent_id)
        .bind(&entry.mime_type)
        .bind(entry.size)
        .bind(&tags_json)
        .bind(&entry.thumb_path)
        .bind(entry.width)
        .bind(entry.height)
        .bind(entry.duration_secs)
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
        INSERT INTO media (name, path, parent_id, mime_type, size, tags, thumb_path, width, height, duration_secs)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ON CONFLICT(path) DO UPDATE SET
            name=excluded.name,
            parent_id=excluded.parent_id,
            mime_type=excluded.mime_type,
            size=excluded.size,
            tags=excluded.tags,
            thumb_path=excluded.thumb_path,
            width=excluded.width,
            height=excluded.height,
            duration_secs=excluded.duration_secs
    "#;

    query(q)
        .bind(&entry.name)
        .bind(&entry.path)
        .bind(entry.parent_id)
        .bind(&entry.mime_type)
        .bind(entry.size)
        .bind(&tags_json)
        .bind(&entry.thumb_path)
        .bind(entry.width)
        .bind(entry.height)
        .bind(entry.duration_secs)
        .execute(&mut **tx)
        .await?;

    let id: i64 = query_scalar("SELECT id FROM media WHERE path = ?1")
        .bind(&entry.path)
        .fetch_one(&mut **tx)
        .await?;

    Ok(id)
}

pub async fn get_media_by_id(pool: SqlitePool, id: i64) -> Result<Option<MediaEntry>, sqlx::Error> {
    let row = sqlx::query_as::<_, (i64, String, String, Option<i64>, Option<String>, Option<i64>, Option<String>, Option<String>, Option<i64>, Option<i64>, Option<i64>, String)>(
        "SELECT id, name, path, parent_id, mime_type, size, tags, thumb_path, width, height, duration_secs, created_at FROM media WHERE id = ?1",
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
            created_at: r.11,
            tags,
            thumb_path: r.7,
            width: r.8,
            height: r.9,
            duration_secs: r.10,
        }))
    } else {
        Ok(None)
    }
}

pub async fn get_media_by_path(
    pool: SqlitePool,
    path: String,
) -> Result<Option<MediaEntry>, sqlx::Error> {
    let row = sqlx::query_as::<_, (i64, String, String, Option<i64>, Option<String>, Option<i64>, Option<String>, Option<String>, Option<i64>, Option<i64>, Option<i64>, String)>(
        "SELECT id, name, path, parent_id, mime_type, size, tags, thumb_path, width, height, duration_secs, created_at FROM media WHERE path = ?1",
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
            created_at: r.11,
            tags,
            thumb_path: r.7,
            width: r.8,
            height: r.9,
            duration_secs: r.10,
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
    let rows = sqlx::query_as::<_, (i64, String, String, Option<i64>, Option<String>, Option<i64>, Option<String>, Option<String>, Option<i64>, Option<i64>, Option<i64>, String)>(
        "SELECT id, name, path, parent_id, mime_type, size, tags, thumb_path, width, height, duration_secs, created_at FROM media WHERE parent_id IS ?1",
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
            created_at: r.11,
            tags: tags_vec,
            thumb_path: r.7,
            width: r.8,
            height: r.9,
            duration_secs: r.10,
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

pub async fn list_children_advanced(
    pool: SqlitePool,
    parent_id: Option<i64>,
    tags: Option<Vec<String>>, // if provided, we'll post-filter in Rust and paginate after filtering
    type_filter: Option<&str>, // "file" | "directory"
    kind_filter: Option<&str>, // "image" | "video" | "audio" | "other"
    limit: Option<i64>,
    offset: Option<i64>,
    sort: Option<&str>,  // "name" | "created" | "size"
    order: Option<&str>, // "asc" | "desc"
) -> Result<Vec<MediaEntry>, sqlx::Error> {
    // Build dynamic SQL safely by mapping only known parameters to SQL fragments.
    let mut sql = String::from(
        "SELECT id, name, path, parent_id, mime_type, size, tags, thumb_path, width, height, duration_secs, created_at FROM media WHERE parent_id IS ?",
    );

    // Type filter
    if let Some(t) = type_filter {
        match t {
            "file" => sql.push_str(" AND mime_type IS NOT NULL"),
            "directory" => sql.push_str(" AND mime_type IS NULL"),
            _ => {}
        }
    }

    // Kind filter
    if let Some(k) = kind_filter {
        match k {
            "image" => sql.push_str(" AND mime_type LIKE 'image/%'"),
            "video" => sql.push_str(" AND mime_type LIKE 'video/%'"),
            "audio" => sql.push_str(" AND mime_type LIKE 'audio/%'"),
            "other" => sql.push_str(
                " AND mime_type IS NOT NULL AND mime_type NOT LIKE 'image/%' AND mime_type NOT LIKE 'video/%' AND mime_type NOT LIKE 'audio/%'",
            ),
            _ => {}
        }
    }

    // Sorting
    let sort_col = match sort.unwrap_or("name") {
        "name" => "name",
        "created" | "created_at" => "created_at",
        "size" => "size",
        _ => "name",
    };
    let ord = match order.unwrap_or("asc").to_ascii_lowercase().as_str() {
        "asc" => "ASC",
        "desc" => "DESC",
        _ => "ASC",
    };
    sql.push_str(&format!(" ORDER BY {} {}", sort_col, ord));

    let use_sql_pagination = tags.is_none();
    if use_sql_pagination {
        // Apply LIMIT/OFFSET only when we are not filtering by tags at the Rust layer.
        let lim = limit.unwrap_or(100).max(0);
        let off = offset.unwrap_or(0).max(0);
        sql.push_str(" LIMIT ? OFFSET ?");

        let q = sqlx::query_as::<
            _,
            (
                i64,
                String,
                String,
                Option<i64>,
                Option<String>,
                Option<i64>,
                Option<String>,
                Option<String>,
                Option<i64>,
                Option<i64>,
                Option<i64>,
                String,
            ),
        >(&sql)
        .bind(parent_id)
        .bind(lim)
        .bind(off);

        let rows = q.fetch_all(&pool).await?;

        let mut out = Vec::new();
        for r in rows {
            let tags_vec: Option<Vec<String>> =
                r.6.as_ref().and_then(|s| serde_json::from_str(s).ok());
            out.push(MediaEntry {
                id: r.0,
                name: r.1,
                path: r.2,
                parent_id: r.3,
                mime_type: r.4,
                size: r.5,
                created_at: r.11,
                tags: tags_vec,
                thumb_path: r.7,
                width: r.8,
                height: r.9,
                duration_secs: r.10,
            });
        }
        return Ok(out);
    }

    // Without SQL LIMIT/OFFSET, fetch all, filter tags in Rust, then paginate.
    let q = sqlx::query_as::<
        _,
        (
            i64,
            String,
            String,
            Option<i64>,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<i64>,
            Option<i64>,
            String,
        ),
    >(&sql)
    .bind(parent_id);

    let rows = q.fetch_all(&pool).await?;

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
            created_at: r.11,
            tags: tags_vec,
            thumb_path: r.7,
            width: r.8,
            height: r.9,
            duration_secs: r.10,
        });
    }

    let mut filtered = if let Some(filter_tags) = tags {
        out.into_iter()
            .filter(|entry| {
                if let Some(tlist) = &entry.tags {
                    filter_tags.iter().all(|ft| tlist.contains(ft))
                } else {
                    false
                }
            })
            .collect::<Vec<_>>()
    } else {
        out
    };

    // Apply pagination after filtering
    let lim = limit.unwrap_or(100).max(0) as usize;
    let off = offset.unwrap_or(0).max(0) as usize;
    let end = off.saturating_add(lim);
    let sliced = if off >= filtered.len() {
        Vec::new()
    } else if end >= filtered.len() {
        filtered.split_off(off)
    } else {
        filtered[off..end].to_vec()
    };

    Ok(sliced)
}

pub async fn count_children(pool: SqlitePool, parent_id: i64) -> Result<i64, sqlx::Error> {
    let count: i64 = query_scalar("SELECT COUNT(1) FROM media WHERE parent_id = ?1")
        .bind(parent_id)
        .fetch_one(&pool)
        .await?;
    Ok(count)
}
