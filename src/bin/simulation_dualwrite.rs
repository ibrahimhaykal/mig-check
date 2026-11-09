use anyhow::Result;
use serde::Serialize;
use sqlx::postgres::{PgPool, PgPoolOptions};

#[derive(Serialize)]
struct ExpectedIssue {
    id: i64,
    kind: &'static str,
}

async fn reset_table(pool: &PgPool) -> Result<()> {
    sqlx::query("DROP TABLE IF EXISTS sim_orders").execute(pool).await?;
    sqlx::query("CREATE TABLE sim_orders (id BIGINT PRIMARY KEY, amount BIGINT NOT NULL, status TEXT NOT NULL)")
        .execute(pool)
        .await?;
    Ok(())
}

async fn write_order(pool: &PgPool, id: i64, amount: i64) -> Result<()> {
    sqlx::query("INSERT INTO sim_orders (id, amount, status) VALUES ($1, $2, 'paid')")
        .bind(id)
        .bind(amount)
        .execute(pool)
        .await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let source = PgPoolOptions::new()
        .max_connections(4)
        .connect("postgres://postgres:postgres@source:5432/migcheck")
        .await?;
    let target = PgPoolOptions::new()
        .max_connections(4)
        .connect("postgres://postgres:postgres@target:5432/migcheck")
        .await?;

    tokio::try_join!(reset_table(&source), reset_table(&target))?;

    let mut expected = Vec::new();

    for id in 1..=20_000_i64 {
        let amount = id * 10;

        if id % 97 == 0 {
            // Write ke target "gagal"
            write_order(&source, id, amount).await?;
            expected.push(ExpectedIssue { id, kind: "missing_in_target" });
        } else if id % 131 == 0 {
            // Write ke source "gagal"
            write_order(&target, id, amount).await?;
            expected.push(ExpectedIssue { id, kind: "missing_in_source" });
        } else if id % 211 == 0 {
            // Dua write sukses, tapi nilainya beda
            write_order(&source, id, amount).await?;
            write_order(&target, id, amount + 1).await?;
            expected.push(ExpectedIssue { id, kind: "value_mismatch" });
        } else {
            tokio::try_join!(
                write_order(&source, id, amount),
                write_order(&target, id, amount)
            )?;
        }
    }

    std::fs::write("expected.json", serde_json::to_string_pretty(&expected)?)?;
    println!("simulasi selesai: {} failure disuntik", expected.len());

    Ok(())
}