use axum::{
    body::BoxBody,
    http::header,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use axum::extract::State;
use reqwest::StatusCode;
use serde::Deserialize;
use std::str::FromStr;

use tracing::{info, Level};
use tracing_subscriber::{filter::Targets, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone)]
struct ServerState {
    client: reqwest::Client,
}

#[tokio::main]
async fn main() {
    let filter = Targets::from_str(std::env::var("RUST_LOG").as_deref().unwrap_or("info"))
        .expect("RUST_LOG should be a valid tracing filter");
    tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .json()
        .finish()
        .with(filter)
        .init();

    let _guard = sentry::init((std::env::var("SENTRY_DSN").expect("$SENTRY_DSN must be set"), sentry::ClientOptions {
        release: sentry::release_name!(),
        ..Default::default()
    }));

    let state = ServerState {
        client: Default::default(),
    };

    let app = Router::new()
    .route("/", get(root_get))
    .route("/panic", get(|| async { panic!("This is a test panic") }))
    .with_state(state);

    let quit_sig = async {
        _ = tokio::signal::ctrl_c().await;
        warn!("Initiating graceful shutdown");
    };

    let addr = "0.0.0.0:8080".parse().unwrap();
    info!("Listening on {addr}");
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(quit_sig)
        .await
        .unwrap();
}

async fn root_get(State(state): State<ServerState>) -> Response<BoxBody> {
    match get_cat_ascii_art(&state.client).await {
        Ok(art) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            art,
        )
            .into_response(),
        Err(e) => {
            println!("Something went wrong: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong").into_response()
        }
    }
}

async fn get_cat_ascii_art(client: &reqwest::Client) -> color_eyre::Result<String> {
    #[derive(Deserialize)]
    struct CatImage {
        url: String,
    }

    let api_url = "https://api.thecatapi.com/v1/images/search";

    let image = client
        .get(api_url)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<CatImage>>()
        .await?
        .pop()
        .ok_or_else(|| color_eyre::eyre::eyre!("The Cat API returned no images"))?;

    let image_bytes = client
        .get(image.url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec();

    let image = image::load_from_memory(&image_bytes)?;

    let ascii_art = artem::convert(
        image,
        artem::options::OptionBuilder::new()
            .target(artem::options::TargetType::HtmlFile(true, true))
            .build(),
    );

    Ok(ascii_art)
}
