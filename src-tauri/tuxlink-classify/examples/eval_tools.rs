//! Tool-surface corpus eval — the groundwork "shortlist-size chart" of the
//! approved tool-narrowing experiment design (dev/plans/2026-08-10-…).
//!
//! Runs the labeled operator asks against the registry-generated tool
//! corpus (corpus instance #2 on the same machinery as the catalog) and
//! reports: hit@k at k ∈ {5, 8, 12, 16} — the curve that picks the
//! shortlist size and bounds the real miss rate — plus the reject-gap and
//! margin stats that calibrate the two thresholds for this corpus.
//!
//! Usage (same weights layout as eval_floor):
//!
//! ```text
//! TUXLINK_BGE_DIR=~/models/bge-small-en-v1.5 \
//!   cargo run --release --example eval_tools -p tuxlink-classify
//! ```

use serde::Deserialize;
use tuxlink_classify::{CandleBert, CatalogIndex, EmbeddingBackend, Pooling, EMBED_TEMPLATE_VERSION};

const TOOL_CORPUS: &str = include_str!("../../resources/agents/tool-surface.jsonl");
const QUERIES_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../dev/spikes/2026-08-10-tool-surface-embedding/queries.jsonl"
);
const CORPUS: &str = "tuxlink-tools";
const KS: [usize; 4] = [5, 8, 12, 16];

#[derive(Debug, Deserialize)]
struct Expect {
    kind: String,
    #[serde(default)]
    tools: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Query {
    id: String,
    q: String,
    expect: Expect,
    behavior: String,
}

fn median(xs: &mut [f32]) -> f32 {
    if xs.is_empty() {
        return f32::NAN;
    }
    xs.sort_by(|a, b| a.total_cmp(b));
    xs[xs.len() / 2]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::var("TUXLINK_BGE_DIR")
        .map_err(|_| "set TUXLINK_BGE_DIR to the model directory")?;
    let model_id =
        std::env::var("TUXLINK_MODEL_ID").unwrap_or_else(|_| "bge-small-en-v1.5".into());

    let queries: Vec<Query> = std::fs::read_to_string(QUERIES_PATH)?
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| serde_json::from_str(l).map_err(|e| format!("{l}: {e}")))
        .collect::<Result<_, _>>()?;
    let entries = tuxlink_classify::enriched::parse_jsonl(TOOL_CORPUS)?;

    eprintln!("loading {model_id} from {dir}; {} tools, {} queries", entries.len(), queries.len());
    let backend = CandleBert::load(std::path::Path::new(&dir), Pooling::Cls, model_id.clone())?;
    let index = CatalogIndex::build(CORPUS, &entries, &backend)?;

    let mut hits_at = [0u32; 4];
    let mut labeled = 0u32;
    let mut none_top1_max = f32::MIN;
    let mut true_top1_min = f32::MAX;
    let mut answer_margins: Vec<f32> = vec![];
    let mut ask_margins: Vec<f32> = vec![];

    for q in &queries {
        let (cands, margin) = index.rank(&q.q, None, *KS.last().unwrap(), &backend)?;
        let margin_v = margin.unwrap_or(f32::NAN);
        let rank = cands
            .iter()
            .position(|c| q.expect.tools.contains(&c.id))
            .map(|p| p + 1);

        if q.expect.kind == "tool" {
            labeled += 1;
            for (i, k) in KS.iter().enumerate() {
                if matches!(rank, Some(r) if r <= *k) {
                    hits_at[i] += 1;
                }
            }
        }
        match q.expect.kind.as_str() {
            "none" => none_top1_max = none_top1_max.max(cands[0].score),
            _ => true_top1_min = true_top1_min.min(cands[0].score),
        }
        match q.behavior.as_str() {
            "answer" => answer_margins.push(margin_v),
            _ if q.expect.kind != "none" => ask_margins.push(margin_v),
            _ => {}
        }
        println!(
            "{}\t{}\t{}\t{:.4}\t{:.4}\t{}",
            q.id,
            rank.map(|r| r.to_string()).unwrap_or_else(|| "-".into()),
            cands[0].id,
            cands[0].score,
            margin_v,
            cands.iter().take(5).map(|c| c.id.as_str()).collect::<Vec<_>>().join(",")
        );
    }

    let ask_max = ask_margins.iter().copied().fold(f32::MIN, f32::max);
    let ans_min = answer_margins.iter().copied().fold(f32::MAX, f32::min);
    println!("---- shortlist-size chart ({labeled} tool-labeled queries) ----");
    for (i, k) in KS.iter().enumerate() {
        println!(
            "hit@{k}: {}/{labeled} ({:.1}%)",
            hits_at[i],
            100.0 * hits_at[i] as f32 / labeled as f32
        );
    }
    println!(
        "reject gap: none-class max {none_top1_max:.4} vs true-class min {true_top1_min:.4} ({})",
        if none_top1_max < true_top1_min { "SEPARATED" } else { "OVERLAP" }
    );
    println!(
        "margins: answer min {ans_min:.4} median {:.4} | ask max {ask_max:.4} median {:.4}",
        median(&mut answer_margins.clone()),
        median(&mut ask_margins.clone())
    );
    println!("---- paste-ready thresholds (verify SEPARATED first) ----");
    println!(
        "{{\"{CORPUS}/{model_id}/{EMBED_TEMPLATE_VERSION}\":{{\"reject_floor\":{:.3},\"ask_margin\":{:.3}}}}}",
        (none_top1_max + true_top1_min) / 2.0,
        (ask_max + ans_min) / 2.0
    );
    Ok(())
}
