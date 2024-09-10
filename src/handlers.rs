// region:    --- Imports
use crate::bidding::commands::{
    handle_buy_now as command_handle_buy_now, handle_place_bid, BuyNowCommand, PlaceBidCommand,
};
use crate::database::DatabaseManager;
use crate::event_store::PostgresEventStore;
use crate::message_broker::KafkaProducer;
use crate::query;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use std::sync::Arc;
use tracing::info;

// endregion: --- Imports

// region:    --- Command Handlers

/// 입찰 요청 처리
pub async fn handle_bid(
    State((db_manager, kafka_producer)): State<(Arc<DatabaseManager>, Arc<KafkaProducer>)>,
    Json(cmd): Json<PlaceBidCommand>,
) -> impl IntoResponse {
    info!("{:<12} --> 입찰 요청 처리 시작: {:?}", "Command", cmd);

    // 이벤트 저장소 생성
    let event_store = PostgresEventStore::new(Arc::clone(&db_manager), Arc::clone(&kafka_producer));

    let item_id = cmd.item_id;

    // 현재 가격 조회
    let current_price = match query::handlers::get_item_current_price(&db_manager, item_id).await {
        Ok(price) => price,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(e.to_string()),
            )
                .into_response()
        }
    };

    // 입찰 가격이 현재 가격보다 높은지 검증
    if cmd.bid_amount <= current_price {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "입찰 가격은 현재 가격보다 높아야 합니다.",
                "current_price": current_price
            })),
        )
            .into_response();
    }

    let bid_amount = cmd.bid_amount;

    // 입찰 처리
    match handle_place_bid(cmd, &event_store, &db_manager).await {
        Ok(_) => {
            let updated_item = query::handlers::get_item(&db_manager, item_id)
                .await
                .unwrap();
            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({
                    "message": "입찰이 성공적으로 처리되었습니다.",
                    "current_price": updated_item.current_price,
                    "bid_amount": bid_amount
                })),
            )
                .into_response()
        }
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, Json(e)).into_response(),
    }
}

/// 즉시 구매 요청 처리
pub async fn handle_buy_now(
    State((db_manager, kafka_producer)): State<(Arc<DatabaseManager>, Arc<KafkaProducer>)>,
    Json(cmd): Json<BuyNowCommand>,
) -> impl IntoResponse {
    info!("{:<12} --> 즉시 구매 요청 처리 시작: {:?}", "Command", cmd);

    // 이벤트 저장소 생성
    let event_store = PostgresEventStore::new(Arc::clone(&db_manager), Arc::clone(&kafka_producer));
    // 아이템 상태 확인
    let item = match query::handlers::get_item(&db_manager, cmd.item_id).await {
        Ok(item) => item,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to fetch item: {}", e)
                })),
            )
                .into_response()
        }
    };

    // 경매가 아직 시작되지 않았을 경우 예외 처리
    let now = Utc::now();
    if now < item.start_time {
        info!("{:<12} --> 경매가 아직 시작되지 않았습니다.", "Command");
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "경매가 아직 시작되지 않았습니다.",
                "code": "NOT_STARTED"
            })),
        )
            .into_response();
    }

    // 즉시 구매 처리
    match process_buy_now(&db_manager, cmd, &event_store).await {
        Ok(_) => (axum::http::StatusCode::OK, "Buy now executed successfully").into_response(),
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, e).into_response(),
    }
}

/// 즉시 구매 처리 프로세스
async fn process_buy_now(
    db_manager: &DatabaseManager,
    cmd: BuyNowCommand,
    event_store: &PostgresEventStore,
) -> Result<(), String> {
    info!(
        "{:<12} --> 즉시 구매 처리 프로세스 시작: {:?}",
        "Command", cmd
    );
    // 즉시 구매 가격 가져오기
    let buy_now_price = query::handlers::get_item_buy_now_price(db_manager, cmd.item_id)
        .await
        .map_err(|e| e.to_string())?;

    // handle_buy_now 함수 호출
    command_handle_buy_now(cmd, buy_now_price, event_store, db_manager)
        .await
        .map_err(|e| e.to_string())
}

// endregion: --- Command Handlers

// region:    --- Query Handlers

/// 경매 상태 조회
pub async fn handle_get_auction_state(
    State((db_manager, _)): State<(Arc<DatabaseManager>, Arc<KafkaProducer>)>,
    Path(item_id): Path<i64>,
) -> impl IntoResponse {
    info!("{:<12} --> 경매 상태 조회 id: {}", "HandlerQuery", item_id);
    match query::handlers::get_auction_state(&db_manager, item_id).await {
        Ok(item) => Json(item).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// 최고 입찰가 조회
pub async fn handle_get_highest_bid(
    State((db_manager, _)): State<(Arc<DatabaseManager>, Arc<KafkaProducer>)>,
    Path(item_id): Path<i64>,
) -> impl IntoResponse {
    info!(
        "{:<12} --> 최고 입찰가 조회 id: {}",
        "HandlerQuery", item_id
    );
    match query::handlers::get_highest_bid(&db_manager, item_id).await {
        Ok(bid) => Json(bid).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// 입찰 이력 조회
pub async fn handle_get_bid_history(
    State((db_manager, _)): State<(Arc<DatabaseManager>, Arc<KafkaProducer>)>,
    Path(item_id): Path<i64>,
) -> impl IntoResponse {
    info!("{:<12} --> 입찰 이력 조회 id: {}", "HandlerQuery", item_id);
    match query::handlers::get_bid_history(&db_manager, item_id).await {
        Ok(history) => Json(history).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// 모든 상품 조회
pub async fn handle_get_items(
    State((db_manager, _)): State<(Arc<DatabaseManager>, Arc<KafkaProducer>)>,
) -> impl IntoResponse {
    info!("{:<12} --> 모든 상품 조회", "HandlerQuery");
    match query::handlers::get_all_items(&db_manager).await {
        Ok(items) => Json(items).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// 상품 조회
pub async fn handle_get_item(
    State((db_manager, _)): State<(Arc<DatabaseManager>, Arc<KafkaProducer>)>,
    Path(item_id): Path<i64>,
) -> impl IntoResponse {
    info!("{:<12} --> 상품 조회 id: {}", "HandlerQuery", item_id);
    match query::handlers::get_item(&db_manager, item_id).await {
        Ok(item) => Json(item).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// 상품 입찰 이력 조회
pub async fn handle_get_item_bids(
    State((db_manager, _)): State<(Arc<DatabaseManager>, Arc<KafkaProducer>)>,
    Path(item_id): Path<i64>,
) -> impl IntoResponse {
    info!(
        "{:<12} --> 상품 입찰 이력 조회 id: {}",
        "HandlerQuery", item_id
    );
    match query::handlers::get_item_bids(&db_manager, item_id).await {
        Ok(bids) => Json(bids).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// endregion: --- Query Handlers
