// region:    --- Imports
use crate::database::DatabaseManager;
use crate::event_store::EventConsumer;
use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use message_broker::KafkaManager;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info};
// endregion: --- Imports

// region:    --- Modules
mod auction;
mod bidding;
mod database;
mod event_store;
mod handlers;
mod message_broker;
mod query;
mod scheduler;

// endregion: --- Modules

// region:    --- Main
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // logging 초기화
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .without_time()
        .with_target(false)
        .init();

    // DatabaseManager 생성
    let db_manager = Arc::new(DatabaseManager::new().await);

    // 데이터베이스 초기화
    if let Err(e) = db_manager.initialize_database().await {
        error!("{:<12} --> 데이터베이스 초기화 실패: {:?}", "Main", e);
        return Err(e.into());
    }
    info!("{:<12} --> 데이터베이스 초기화 성공", "Main");

    // Kafka 매니저 생성 및 초기화
    let kafka_manager = Arc::new(KafkaManager::new());
    if let Err(e) = kafka_manager.initialize().await {
        error!("{:<12} --> Kafka 초기화 실패: {:?}", "Main", e);
        return Err(e.into());
    }
    info!("{:<12} --> Kafka 초기화 성공", "Main");

    // 토픽 생성
    kafka_manager.create_topic("events", 5, 1).await?;

    // 이벤트 소싱 시작
    let event_consumer = EventConsumer::new(Arc::clone(&db_manager), kafka_manager.get_consumer());
    tokio::spawn(async move {
        event_consumer.start().await;
    });

    // 가상의 상품 (상태) 관리 마이크로 서비스
    let scheduler = scheduler::AuctionScheduler::new(db_manager.get_pool());
    scheduler.start().await;

    // 테스트 페이지를 위한 cors 설정
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // 라우터 설정
    let routes_all = Router::new()
        .route("/bid", post(handlers::handle_bid))
        .route("/buy-now", post(handlers::handle_buy_now))
        .route("/auction/:id", get(handlers::handle_get_auction_state))
        .route(
            "/auction/:id/highest-bid",
            get(handlers::handle_get_highest_bid),
        )
        .route("/auction/:id/bids", get(handlers::handle_get_bid_history))
        .route("/items", get(handlers::handle_get_items))
        .route("/items/:id", get(handlers::handle_get_item))
        .route("/items/:id/bids", get(handlers::handle_get_item_bids))
        .layer(cors)
        .layer(DefaultBodyLimit::max(1024 * 1024 * 20)) // 동시성을 위한 바디 사이즈 10배 증가(20MB)
        .with_state((db_manager, kafka_manager.get_producer()));

    // 리스너 생성(로컬 호스트의 3000번 포트를 사용)
    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    info!(
        "{:<12} --> Web Server: Listening on {}",
        "Main",
        listener.local_addr().unwrap()
    );

    // 서버 실행
    if let Err(err) = axum::serve(listener, routes_all.into_make_service()).await {
        error!("{:<12} --> Server error: {}", "Main", err);
    }
    Ok(())
}
// endregion: --- Main
