mod algorithms;
mod handlers;
mod precision;
mod state;

use actix_cors::Cors;
use actix_web::{web, App, HttpServer, HttpResponse, middleware};
use state::AppState;
use std::sync::Mutex;

fn cors_middleware() -> Cors {
    Cors::default()
        .allow_any_origin()
        .allow_any_method()
        .allow_any_header()
        .max_age(3600)
        .supports_credentials()
}

fn setup_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .route("/pi", web::get().to(handlers::get_pi))
            .route("/pi/{algo}", web::get().to(handlers::get_pi_algorithm))
            .route("/compare", web::get().to(handlers::compare_algorithms))
            .route("/digit/{position}", web::get().to(handlers::get_digit))
            .route("/convergence/{algo}", web::get().to(handlers::stream_convergence))
            .route("/history", web::get().to(handlers::get_history))
            .route("/health", web::get().to(handlers::health_check)),
    );
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let app_state = web::Data::new(Mutex::new(AppState::new()));

    println!("Starting pi-calc-api on http://localhost:3001");

    HttpServer::new(move || {
        App::new()
            .wrap(cors_middleware())
            .app_data(app_state.clone())
            .configure(setup_routes)
    })
    .bind("0.0.0.0:3001")?
    .run()
    .await
}
