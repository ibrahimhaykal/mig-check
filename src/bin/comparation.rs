use anyhow::Result;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Deserialize, Hash, PartialEq, Eq, Debug)]
struct Issue {
    id: i64,
    kind: String,
}

#[derive(Deserialize)]
struct Report {
    issues: Vec<Issue>,
}

fn main() -> Result<()> {
    let expected: Vec<Issue> =
        serde_json::from_str(&std::fs::read_to_string("expected.json")?)?;
    let report: Report =
        serde_json::from_str(&std::fs::read_to_string("report.json")?)?;

    let expected: HashSet<Issue> = expected.into_iter().collect();
    let actual: HashSet<Issue> = report.issues.into_iter().collect();

    let terlewat: Vec<&Issue> = expected.difference(&actual).collect();
    let false_positive: Vec<&Issue> = actual.difference(&expected).collect();

    println!("expected: {}", expected.len());
    println!("ditemukan: {}", actual.len());
    println!("terlewat: {}", terlewat.len());
    println!("false positive: {}", false_positive.len());

    if !terlewat.is_empty() || !false_positive.is_empty() {
        println!("contoh terlewat: {:?}", terlewat.iter().take(5).collect::<Vec<_>>());
        println!("contoh false positive: {:?}", false_positive.iter().take(5).collect::<Vec<_>>());
        anyhow::bail!("hasil tidak cocok");
    }

    println!("COCOK 100%");
    Ok(())
}