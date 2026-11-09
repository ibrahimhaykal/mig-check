use anyhow::Result;
use clap::Parser;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPool, PgPoolOptions};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum IssueKind {
    MissingInTarget,
    MissingInSource,
    ValueMismatch,
}

#[derive(Serialize, Deserialize)]
struct Issue {
    id: i64,
    kind: IssueKind,
}

#[derive(Serialize)]
struct Report {
    table: String,
    source_rows: i64,
    target_rows: i64,
    issues: Vec<Issue>,
}

#[derive(Serialize, Deserialize)]
struct Checkpoint {
    next_start: i64,
    issues: Vec<Issue>,
}

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "postgres://postgres:postgres@source:5432/migcheck")]
    source: String,
    #[arg(long, default_value = "postgres://postgres:postgres@target:5432/migcheck")]
    target: String,
    #[arg(long, default_value = "orders")]
    table: String,
    #[arg(long)]
    resume: bool,
    #[arg(long, default_value_t = 10_000)]
    chunk_size: i64,
    #[arg(long, default_value_t = 0)]
    delay_ms: u64,
}

async fn count_rows(pool: &PgPool, table: &str) -> Result<i64> {
    if table.is_empty() || !table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        anyhow::bail!("nama tabel tidak valid: {table}");
    }

    let query = format!("SELECT COUNT(*) FROM {table}");

    let (count,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(query))
        .fetch_one(pool)
        .await?;

    Ok(count)
}

async fn checksum_range(
    pool: &PgPool,
    table: &str,
    start: i64,
    end: i64,
) -> Result<(i64, Option<String>)> {
    let query = format!(
        "select count(*), md5(string_agg(md5(t::text), '' order by id))
        from {table} t
        where id between {start} and {end}"
    );

    let (count, hash): (i64, Option<String>) = sqlx::query_as(sqlx::AssertSqlSafe(query))
        .fetch_one(pool)
        .await?;

    Ok((count, hash))
}

async fn id_bounds(pool: &PgPool, table: &str) -> Result<(Option<i64>, Option<i64>)> {
    let query = format!("select min(id)::bigint, max(id)::bigint from {table}");

    let (min_id, max_id): (Option<i64>, Option<i64>) = sqlx::query_as(sqlx::AssertSqlSafe(query))
        .fetch_one(pool)
        .await?;

    Ok((min_id, max_id))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if args.chunk_size < 1 {
        anyhow::bail!("chunk_size minimal 1");
    }
    let source_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&args.source)
        .await?;
    let target_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&args.target)
        .await?;

    let (source, target) = tokio::try_join!(
        count_rows(&source_pool, &args.table),
        count_rows(&target_pool, &args.table)
    )?;

    let (source_bounds, target_bounds) = tokio::try_join!(
        id_bounds(&source_pool, &args.table),
        id_bounds(&target_pool, &args.table)
    )?;

    let min_id = [source_bounds.0, target_bounds.0].into_iter().flatten().min();
    let max_id = [source_bounds.1, target_bounds.1].into_iter().flatten().max();

    let (Some(min_id), Some(max_id)) = (min_id, max_id) else {
        println!("tidak ada data di kedua database");
        return Ok(());
    };

    let checkpoint_path = format!("checkpoint_{}.json", args.table);

    let checkpoint_tmp = format!("{checkpoint_path}.tmp");

    let mut checkpoint = if args.resume && std::path::Path::new(&checkpoint_path).exists() {
        let isi = std::fs::read_to_string(&checkpoint_path)?;
        let data: Checkpoint = serde_json::from_str(&isi)?;
        println!("lanjut dari id {}", data.next_start);
        data
    } else {
        Checkpoint {
            next_start: min_id,
            issues: Vec::new(),
        }
    };

    while checkpoint.next_start <= max_id {
        let chunk_start = checkpoint.next_start;
        let chunk_end = chunk_start + args.chunk_size - 1;
        let mut antrian_range = vec![(chunk_start, chunk_end)];

        while let Some((start, end)) = antrian_range.pop() {
            let (hasil_source, hasil_target) = tokio::try_join!(
                checksum_range(&source_pool, &args.table, start, end),
                checksum_range(&target_pool, &args.table, start, end)
            )?;

            if hasil_source != hasil_target {
                if start == end {
                    let kind = if hasil_source.0 == 1 && hasil_target.0 == 0 {
                        IssueKind::MissingInTarget
                    } else if hasil_source.0 == 0 && hasil_target.0 == 1 {
                        IssueKind::MissingInSource
                    } else {
                        IssueKind::ValueMismatch
                    };

                    checkpoint.issues.push(Issue { id: start, kind });
                } else {
                    let mid = (start + end) / 2;
                    antrian_range.push((start, mid));
                    antrian_range.push((mid + 1, end));
                }
            }
        }

        checkpoint.next_start = chunk_end + 1;
        std::fs::write(&checkpoint_tmp, serde_json::to_string(&checkpoint)?)?;
        std::fs::rename(&checkpoint_tmp, &checkpoint_path)?;
        tokio::time::sleep(std::time::Duration::from_millis(args.delay_ms)).await;
    }
    let report = Report {
        table: args.table.clone(),
        source_rows: source,
        target_rows: target,
        issues: checkpoint.issues,
    };
    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write("report.json", &json)?;
    if std::path::Path::new(&checkpoint_path).exists() {
        std::fs::remove_file(&checkpoint_path)?;
    }
    println!("{json}");

    Ok(())
}