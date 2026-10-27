use crate::models::{AppSettings, NvidiaModelItem};
use crate::nvidia::NvidiaClient;

pub struct RankedCandidate {
    pub model: NvidiaModelItem,
    pub score: f64,
}

pub fn select_and_rank_models(
    available_ids: &[String],
    settings: &AppSettings,
) -> Vec<NvidiaModelItem> {
    let catalog = NvidiaClient::known_model_catalog();
    let mut candidates: Vec<NvidiaModelItem> = Vec::new();

    for item in catalog {
        // If available_ids is empty (e.g. offline preview), include all catalog items
        if !available_ids.is_empty() && !available_ids.contains(&item.id) {
            continue;
        }

        // Apply include/exclude rules
        if !settings.include_models.is_empty() && !settings.include_models.contains(&item.id) {
            continue;
        }
        if settings.exclude_models.contains(&item.id) {
            continue;
        }

        // Apply min_context_length filter
        if item.context_length < settings.min_context_length {
            continue;
        }

        candidates.push(item);
    }

    // Sort by benchmark SWE-bench, then MMLU, then context length
    candidates.sort_by(|a, b| {
        let score_a = a.benchmark_swe.unwrap_or(0.0) * 0.6 + a.benchmark_mmlu.unwrap_or(0.0) * 0.4;
        let score_b = b.benchmark_swe.unwrap_or(0.0) * 0.6 + b.benchmark_mmlu.unwrap_or(0.0) * 0.4;
        score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    if candidates.len() > settings.candidate_pool {
        candidates.truncate(settings.candidate_pool);
    }

    candidates
}
