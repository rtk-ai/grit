use actix_web::{web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Instant;

use crate::algorithms;
use crate::precision;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct PiQuery {
    iterations: Option<u64>,
    digits: Option<u32>,
}

#[derive(Serialize)]
struct PiResponse {
    value: f64,
    formatted: String,
    algorithm: String,
    iterations: u64,
    correct_digits: u32,
    error: f64,
    elapsed_ms: f64,
}

#[derive(Serialize)]
struct ComparisonEntry {
    algorithm: String,
    value: f64,
    correct_digits: u32,
    error: f64,
    elapsed_ms: f64,
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    uptime_seconds: u64,
    cache_size: usize,
}

pub async fn get_pi(
    query: web::Query<PiQuery>,
    state: web::Data<Mutex<AppState>>,
) -> HttpResponse {
    let iterations = query.iterations.unwrap_or(100_000);
    let digits = query.digits.unwrap_or(10);

    let start = Instant::now();
    let value = algorithms::leibniz(iterations);
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    let formatted = precision::to_decimal_string(value, digits);
    let correct_digits = precision::significant_digits(value);
    let error = precision::compare_with_known(value);

    let mut app_state = state.lock().unwrap();
    app_state.cache_result("leibniz", iterations, value);

    HttpResponse::Ok().json(PiResponse {
        value,
        formatted,
        algorithm: "leibniz".to_string(),
        iterations,
        correct_digits,
        error,
        elapsed_ms: elapsed,
    })
}

pub async fn get_pi_algorithm(
    path: web::Path<String>,
    query: web::Query<PiQuery>,
    state: web::Data<Mutex<AppState>>,
) -> HttpResponse {
    let algo = path.into_inner();
    let iterations = query.iterations.unwrap_or(10_000);
    let digits = query.digits.unwrap_or(10);

    if let Some(cached) = state.lock().unwrap().get_cached(&algo, iterations) {
        let formatted = precision::to_decimal_string(cached, digits);
        return HttpResponse::Ok().json(PiResponse {
            value: cached,
            formatted,
            algorithm: algo,
            iterations,
            correct_digits: precision::significant_digits(cached),
            error: precision::compare_with_known(cached),
            elapsed_ms: 0.0,
        });
    }

    let start = Instant::now();
    let value = match algo.as_str() {
        "leibniz" => algorithms::leibniz(iterations),
        "monte_carlo" => algorithms::monte_carlo(iterations),
        "nilakantha" => algorithms::nilakantha(iterations),
        "chudnovsky" => algorithms::chudnovsky(iterations),
        "wallis" => algorithms::wallis(iterations),
        "ramanujan" => algorithms::ramanujan(iterations),
        "bbp" => algorithms::bbp(iterations),
        "gauss_legendre" => algorithms::gauss_legendre(iterations),
        _ => return HttpResponse::BadRequest().json(serde_json::json!({"error": "Unknown algorithm"})),
    };
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    let formatted = precision::to_decimal_string(value, digits);
    let correct_digits = precision::significant_digits(value);
    let error = precision::compare_with_known(value);

    state.lock().unwrap().cache_result(&algo, iterations, value);

    HttpResponse::Ok().json(PiResponse {
        value,
        formatted,
        algorithm: algo,
        iterations,
        correct_digits,
        error,
        elapsed_ms: elapsed,
    })
}

pub async fn compare_algorithms(
    query: web::Query<PiQuery>,
    state: web::Data<Mutex<AppState>>,
) -> HttpResponse {
    let iterations = query.iterations.unwrap_or(10_000);
    let algos: Vec<(&str, fn(u64) -> f64)> = vec![
        ("leibniz", algorithms::leibniz),
        ("monte_carlo", algorithms::monte_carlo),
        ("nilakantha", algorithms::nilakantha),
        ("chudnovsky", algorithms::chudnovsky),
        ("wallis", algorithms::wallis),
        ("ramanujan", algorithms::ramanujan),
        ("bbp", algorithms::bbp),
        ("gauss_legendre", algorithms::gauss_legendre),
    ];

    let mut results: Vec<ComparisonEntry> = Vec::new();

    for (name, func) in &algos {
        let start = Instant::now();
        let value = func(iterations);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;

        let correct_digits = precision::significant_digits(value);
        let error = precision::compare_with_known(value);

        state.lock().unwrap().cache_result(name, iterations, value);

        results.push(ComparisonEntry {
            algorithm: name.to_string(),
            value,
            correct_digits,
            error,
            elapsed_ms: elapsed,
        });
    }

    results.sort_by(|a, b| b.correct_digits.cmp(&a.correct_digits));

    HttpResponse::Ok().json(results)
}

pub async fn get_digit(
    path: web::Path<u64>,
) -> HttpResponse {
    let position = path.into_inner();
    let known_pi = "3141592653589793238462643383279502884197169399375105820974944592307816406286208998628034825342117067982148086513282306647093844609550582231725359408128481";

    if position as usize >= known_pi.len() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Position out of range",
            "max_position": known_pi.len() - 1
        }));
    }

    let digit = known_pi.chars().nth(position as usize).unwrap();
    let error_bound = precision::error_bound("bbp", position);

    HttpResponse::Ok().json(serde_json::json!({
        "position": position,
        "digit": digit.to_string(),
        "error_bound": error_bound
    }))
}

pub async fn stream_convergence(
    path: web::Path<String>,
    _req: HttpRequest,
) -> HttpResponse {
    let algo = path.into_inner();
    let max_iterations = 100_u64;
    let mut events = String::new();

    for i in 1..=max_iterations {
        let value = match algo.as_str() {
            "leibniz" => algorithms::leibniz(i * 100),
            "nilakantha" => algorithms::nilakantha(i * 10),
            "wallis" => algorithms::wallis(i * 100),
            "bbp" => algorithms::bbp(i),
            _ => algorithms::leibniz(i * 100),
        };

        let correct = precision::significant_digits(value);
        let error = precision::compare_with_known(value);
        let data = serde_json::json!({
            "iteration": i,
            "value": value,
            "correct_digits": correct,
            "error": error
        });

        events.push_str(&format!("data: {}\n\n", data));
    }

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .body(events)
}

pub async fn get_history(
    state: web::Data<Mutex<AppState>>,
) -> HttpResponse {
    let app_state = state.lock().unwrap();
    let stats = app_state.get_stats();

    HttpResponse::Ok().json(serde_json::json!({
        "total_calculations": stats.total_calculations,
        "cache_hits": stats.cache_hits,
        "algorithms_used": stats.algorithms_used,
        "last_calculation": stats.last_calculation
    }))
}

pub async fn health_check(
    state: web::Data<Mutex<AppState>>,
) -> HttpResponse {
    let app_state = state.lock().unwrap();
    let stats = app_state.get_stats();

    HttpResponse::Ok().json(HealthResponse {
        status: "healthy".to_string(),
        uptime_seconds: 0,
        cache_size: stats.total_calculations,
    })
}
