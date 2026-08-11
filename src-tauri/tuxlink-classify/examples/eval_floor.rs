//! The 44-query floor eval + threshold calibration for the T1 request
//! classifier (ch3e9 step-2 validation, ADR 0030).
//!
//! Runs the labeled query set from the 2026-08-09 T1 embedding spike
//! against the enriched full-catalog index through the NATIVE candle
//! backend — so a run of this binary is simultaneously:
//!
//! 1. the retrieval-accuracy floor (top-1 item/section),
//! 2. the calibration run that measures the reject gap and margin classes
//!    and prints a paste-ready thresholds JSON (values are MEASURED here,
//!    never invented — ADR 0030), and
//! 3. the "confirming candle spike" the ADR requires before quoting native
//!    runtime numbers as fact (index build + per-query encode timings).
//!
//! Usage (weights dir = HF layout: config.json, tokenizer.json,
//! model.safetensors):
//!
//! ```text
//! TUXLINK_BGE_DIR=~/models/bge-small-en-v1.5 \
//!   cargo run --release --example eval_floor -p tuxlink-classify
//! ```
//!
//! Optional: TUXLINK_MODEL_ID (default bge-small-en-v1.5),
//! TUXLINK_POOLING=cls|mean (default cls — bge is a CLS-pooling family).

use std::time::Instant;

use serde::Deserialize;
use tuxlink_classify::{
    embed_text, CandleBert, CatalogIndex, EmbeddingBackend, Pooling, EMBED_TEMPLATE_VERSION,
    WINLINK_CATALOG_CORPUS,
};

const ENRICHED: &str = include_str!("../../resources/catalog/winlink-catalog-enriched.jsonl");
const QUERIES_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../dev/spikes/2026-08-09-t1-catalog-embedding/queries.jsonl"
);

#[derive(Debug, Deserialize)]
struct Expect {
    kind: String,
    #[serde(default)]
    section: Option<String>,
    #[serde(default)]
    items: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Query {
    id: String,
    q: String,
    expect: Expect,
    behavior: String,
}

fn median(xs: &mut Vec<f32>) -> f32 {
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
    let pooling = match std::env::var("TUXLINK_POOLING").as_deref() {
        Ok("mean") => Pooling::Mean,
        _ => Pooling::Cls,
    };

    let queries: Vec<Query> = std::fs::read_to_string(QUERIES_PATH)?
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| serde_json::from_str(l).map_err(|e| format!("{l}: {e}")))
        .collect::<Result<_, _>>()?;
    let entries = tuxlink_classify::enriched::parse_jsonl(ENRICHED)?;

    eprintln!("loading {model_id} ({pooling:?}) from {dir}");
    let t0 = Instant::now();
    let backend = CandleBert::load(std::path::Path::new(&dir), pooling, model_id.clone())?;
    let load_ms = t0.elapsed().as_millis();

    // Warm-up encode outside all timings (first candle forward pays
    // one-time allocation/layout costs that would skew the medians).
    backend.embed(&[embed_text(&entries[0])])?;

    let t0 = Instant::now();
    let index = CatalogIndex::build(WINLINK_CATALOG_CORPUS, &entries, &backend)?;
    let build_ms = t0.elapsed().as_millis();

    // ---- per-query ranking -------------------------------------------------
    let (mut item_hits, mut item_total) = (0u32, 0u32);
    let (mut sec_hits, mut sec_total) = (0u32, 0u32);
    let mut none_top1_max = f32::MIN;
    let mut true_top1_min = f32::MAX;
    let mut answer_margins: Vec<f32> = vec![];
    let mut ask_margins: Vec<f32> = vec![];
    let mut encode_ms: Vec<f32> = vec![];
    let mut rows = String::new();

    for q in &queries {
        let t = Instant::now();
        let (cands, margin) = index.rank(&q.q, None, 5, &backend)?;
        encode_ms.push(t.elapsed().as_secs_f32() * 1e3);
        let top = &cands[0];
        let margin_v = margin.unwrap_or(f32::NAN);

        let (scored, hit) = match q.expect.kind.as_str() {
            "item" => (true, q.expect.items.iter().any(|i| *i == top.id)),
            "section" => (true, q.expect.section.as_deref() == Some(top.section.as_str())),
            _ => (false, false),
        };
        if scored {
            if q.expect.kind == "item" {
                item_total += 1;
                item_hits += u32::from(hit);
            } else {
                sec_total += 1;
                sec_hits += u32::from(hit);
            }
        }
        match q.expect.kind.as_str() {
            "none" => none_top1_max = none_top1_max.max(top.score),
            _ => true_top1_min = true_top1_min.min(top.score),
        }
        match q.behavior.as_str() {
            "answer" => answer_margins.push(margin_v),
            _ if q.expect.kind != "none" => ask_margins.push(margin_v),
            _ => {}
        }
        rows.push_str(&format!(
            "{}\t{}\t{}\t{:.4}\t{:.4}\t{}\n",
            q.id,
            if scored {
                if hit { "HIT" } else { "MISS" }
            } else {
                q.expect.kind.as_str()
            },
            top.id,
            top.score,
            margin_v,
            cands
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>()
                .join(",")
        ));
    }

    // ---- locality experiment (T1 spike design-story query) -----------------
    let (bare, bare_m) = index.rank("pull the weather for my local area", None, 3, &backend)?;
    let (loc, loc_m) = index.rank(
        "pull the weather for my local area",
        Some("grid DM43, Arizona, United States"),
        3,
        &backend,
    )?;

    // ---- suggested thresholds (measured, with overlap warnings) ------------
    let reject_floor = (none_top1_max + true_top1_min) / 2.0;
    let ask_max = ask_margins.iter().copied().fold(f32::MIN, f32::max);
    let ans_min = answer_margins.iter().copied().fold(f32::MAX, f32::min);
    let ask_margin = (ask_max + ans_min) / 2.0;

    print!("{rows}");
    println!("---- floor summary ({} queries) ----", queries.len());
    println!(
        "item top-1: {item_hits}/{item_total}   section top-1: {sec_hits}/{sec_total}"
    );
    println!(
        "reject gap: none-class max {none_top1_max:.4} vs true-class min {true_top1_min:.4} \
         ({})",
        if none_top1_max < true_top1_min { "SEPARATED" } else { "OVERLAP — floor not clean" }
    );
    println!(
        "margins: answer-class min {ans_min:.4} median {:.4} | ask-class max {ask_max:.4} \
         median {:.4} ({})",
        median(&mut answer_margins.clone()),
        median(&mut ask_margins.clone()),
        if ask_max < ans_min { "SEPARATED" } else { "OVERLAP — expect verdict misses" }
    );
    println!("---- locality experiment ----");
    println!(
        "bare:     top {} ({:.4}) margin {:.4} [{}]",
        bare[0].id,
        bare[0].score,
        bare_m.unwrap_or(f32::NAN),
        bare.iter().map(|c| c.section.as_str()).collect::<Vec<_>>().join(",")
    );
    println!(
        "locality: top {} ({:.4}) margin {:.4} [{}]",
        loc[0].id,
        loc[0].score,
        loc_m.unwrap_or(f32::NAN),
        loc.iter().map(|c| c.section.as_str()).collect::<Vec<_>>().join(",")
    );
    println!("---- native runtime (the ADR's confirming-candle numbers) ----");
    println!(
        "model load {load_ms}ms | index build {build_ms}ms for {} items \
         ({:.1}ms/item) | per-query encode median {:.1}ms",
        entries.len(),
        build_ms as f32 / entries.len() as f32,
        median(&mut encode_ms)
    );
    println!("---- paste-ready thresholds (verify SEPARATED above first) ----");
    println!(
        "{{\"{WINLINK_CATALOG_CORPUS}/{model_id}/{EMBED_TEMPLATE_VERSION}\":\
         {{\"reject_floor\":{reject_floor:.3},\"ask_margin\":{ask_margin:.3}}}}}"
    );
    Ok(())
}
