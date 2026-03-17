use std::collections::HashMap;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
struct CacheEntry {
    value: f64,
    iterations: u64,
    calculated_at: DateTime<Utc>,
}

pub struct CacheStats {
    pub total_calculations: usize,
    pub cache_hits: usize,
    pub algorithms_used: Vec<String>,
    pub last_calculation: Option<String>,
}

pub struct AppState {
    cache: HashMap<String, Vec<CacheEntry>>,
    hit_count: usize,
    last_algorithm: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            cache: HashMap::new(),
            hit_count: 0,
            last_algorithm: None,
        }
    }

    pub fn cache_result(&mut self, algorithm: &str, iterations: u64, value: f64) {
        let entry = CacheEntry {
            value,
            iterations,
            calculated_at: Utc::now(),
        };

        self.cache
            .entry(algorithm.to_string())
            .or_insert_with(Vec::new)
            .push(entry);

        self.last_algorithm = Some(algorithm.to_string());
    }

    pub fn get_cached(&mut self, algorithm: &str, iterations: u64) -> Option<f64> {
        if let Some(entries) = self.cache.get(algorithm) {
            for entry in entries.iter().rev() {
                if entry.iterations == iterations {
                    self.hit_count += 1;
                    return Some(entry.value);
                }
            }
        }
        None
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.hit_count = 0;
        self.last_algorithm = None;
    }

    pub fn get_stats(&self) -> CacheStats {
        let total_calculations: usize = self.cache.values().map(|v| v.len()).sum();
        let algorithms_used: Vec<String> = self.cache.keys().cloned().collect();
        let last_calculation = self.last_algorithm.clone();

        CacheStats {
            total_calculations,
            cache_hits: self.hit_count,
            algorithms_used,
            last_calculation,
        }
    }
}
