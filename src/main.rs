use anyhow::Result;
use clap::Parser;
use serde::Serialize;
use sqlx::postgres::{PgPool, PgPoolOptions};

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum IssueKind {
    MissingInTarget,
    MissingInSource,
    ValueMismatch,
}

#[derive(Serialize)]
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

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "postgres://postgres:postgres@source:5432/migcheck")]
    source: String,
    #[arg(long, default_value = "postgres://postgres:postgres@target:5432/migcheck")]
    target: String,
    #[arg(long, default_value = "orders")]
    table: String,
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

    let mut antrian_range: Vec<(i64, i64)> = Vec::new();

    for start in (min_id..=max_id).step_by(10_000) {
        let end = start + 9_999;

        let (hasil_source, hasil_target) = tokio::try_join!(
            checksum_range(&source_pool, &args.table, start, end),
            checksum_range(&target_pool, &args.table, start, end)
        )?;

        if hasil_source != hasil_target {
            println!("beda di range {start}..{end}");
            antrian_range.push((start, end));
        }
    }

    let mut issues: Vec<Issue> = Vec::new();

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

                issues.push(Issue { id: start, kind });
            } else {
                let mid = (start + end) / 2;
                antrian_range.push((start, mid));
                antrian_range.push((mid + 1, end));
            }
        }
    }

    let report = Report {
        table: args.table.clone(),
        source_rows: source,
        target_rows: target,
        issues,
    };

    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write("report.json", &json)?;
    println!("{json}");

    Ok(())
}