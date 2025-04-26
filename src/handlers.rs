// region:    --- Imports
use crate::bidding::commands::{
    handle_buy_now as command_handle_buy_now, handle_place_bid, BuyNowCommand, PlaceBidCommand,
};
use crate::bidding::model::{Bid, Item};
use crate::database::DatabaseManager;
use crate::error::{AppError, Result};
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
                Json(serde_json::json!({
                    "code": "DATABASE_ERROR",
                    "message": "데이터베이스 오류가 발생했습니다",
                    "details": e.to_string()
                })),
            )
                .into_response()
        }
    };

    // 입찰 가격이 현재 가격보다 높은지 검증
    if cmd.bid_amount <= current_price {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "code": "LOW_BID",
                "message": "입찰 가격은 현재 가격보다 높아야 합니다.",
                "details": {
                    "current_price": current_price,
                    "bid_amount": cmd.bid_amount
                }
            })),
        )
            .into_response();
    }

    let bid_amount = cmd.bid_amount;

    // 입찰 처리
    match handle_place_bid(cmd, &event_store, &db_manager).await {
        Ok(_) => {
            let updated_item = match query::handlers::get_item(&db_manager, item_id).await {
                Ok(item) => item,
                Err(e) => {
                    return (
                        axum::http::StatusCode::OK,
                        Json(serde_json::json!({
                            "message": "입찰이 성공적으로 처리되었으나, 업데이트된 정보를 가져오는데 실패했습니다.",
                            "error": e.to_string()
                        })),
                    )
                        .into_response()
                }
            };

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
        Err(e) => {
            let (status, message) = match e {
                AppError::NotFound(_) => (
                    axum::http::StatusCode::NOT_FOUND,
                    "요청한 경매 아이템을 찾을 수 없습니다.",
                ),
                AppError::InvalidState(_) => (
                    axum::http::StatusCode::BAD_REQUEST,
                    "경매 상태가 유효하지 않습니다.",
                ),
                AppError::Validation(_, _) => (
                    axum::http::StatusCode::BAD_REQUEST,
                    "입력 값이 유효하지 않습니다.",
                ),
                AppError::Conflict(_) => (
                    axum::http::StatusCode::CONFLICT,
                    "동시성 충돌이 발생했습니다. 다시 시도해주세요.",
                ),
                _ => (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "서버 내부 오류가 발생했습니다.",
                ),
            };

            (
                status,
                Json(serde_json::json!({
                    "code": match e {
                        AppError::NotFound(_) => "NOT_FOUND",
                        AppError::InvalidState(_) => "INVALID_STATE",
                        AppError::Validation(_, _) => "VALIDATION_ERROR",
                        AppError::Conflict(_) => "CONFLICT",
                        AppError::Database(_) => "DATABASE_ERROR",
                        AppError::Kafka(_) => "KAFKA_ERROR",
                        AppError::Internal(_) => "INTERNAL_ERROR",
                    },
                    "message": message,
                    "details": e.to_string()
                })),
            )
                .into_response()
        }
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
                    "code": "DATABASE_ERROR",
                    "message": "데이터베이스 오류가 발생했습니다",
                    "details": e.to_string()
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
                "code": "NOT_STARTED",
                "message": "경매가 아직 시작되지 않았습니다.",
                "details": {
                    "start_time": item.start_time,
                    "current_time": now
                }
            })),
        )
            .into_response();
    }

    // 즉시 구매 처리
    let item_id = cmd.item_id; // 미리 item_id 저장
    match process_buy_now(&db_manager, &cmd, &event_store).await {
        Ok(_) => {
            let updated_item = match query::handlers::get_item(&db_manager, item_id).await {
                Ok(item) => item,
                Err(e) => {
                    return (
                        axum::http::StatusCode::OK,
                        Json(serde_json::json!({
                            "code": "SUCCESS",
                            "message": "즉시 구매가 성공적으로 처리되었으나, 업데이트된 정보를 가져오는데 실패했습니다.",
                            "error": e.to_string()
                        })),
                    )
                        .into_response()
                }
            };

            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({
                    "code": "SUCCESS",
                    "message": "즉시 구매가 성공적으로 처리되었습니다.",
                    "data": {
                        "item_id": updated_item.id,
                        "final_price": updated_item.current_price,
                        "status": updated_item.status
                    }
                })),
            )
                .into_response()
        }
        Err(e) => {
            let (status, message) = match e {
                AppError::NotFound(_) => (
                    axum::http::StatusCode::NOT_FOUND,
                    "요청한 경매 아이템을 찾을 수 없습니다.",
                ),
                AppError::InvalidState(_) => (
                    axum::http::StatusCode::BAD_REQUEST,
                    "경매 상태가 유효하지 않습니다.",
                ),
                AppError::Validation(_, _) => (
                    axum::http::StatusCode::BAD_REQUEST,
                    "입력 값이 유효하지 않습니다.",
                ),
                AppError::Conflict(_) => (
                    axum::http::StatusCode::CONFLICT,
                    "동시성 충돌이 발생했습니다. 다시 시도해주세요.",
                ),
                _ => (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "서버 내부 오류가 발생했습니다.",
                ),
            };

            (
                status,
                Json(serde_json::json!({
                    "code": match e {
                        AppError::NotFound(_) => "NOT_FOUND",
                        AppError::InvalidState(_) => "INVALID_STATE",
                        AppError::Validation(_, _) => "VALIDATION_ERROR",
                        AppError::Conflict(_) => "CONFLICT",
                        AppError::Database(_) => "DATABASE_ERROR",
                        AppError::Kafka(_) => "KAFKA_ERROR",
                        AppError::Internal(_) => "INTERNAL_ERROR",
                    },
                    "message": message,
                    "details": e.to_string()
                })),
            )
                .into_response()
        }
    }
}

/// 즉시 구매 처리 프로세스
async fn process_buy_now(
    db_manager: &DatabaseManager,
    cmd: &BuyNowCommand,
    event_store: &PostgresEventStore,
) -> Result<()> {
    info!(
        "{:<12} --> 즉시 구매 처리 프로세스 시작: {:?}",
        "Command", cmd
    );
    // 즉시 구매 가격 가져오기
    let buy_now_price = query::handlers::get_item_buy_now_price(db_manager, cmd.item_id).await?;

    // handle_buy_now 함수 호출
    command_handle_buy_now(cmd.clone(), buy_now_price, event_store, db_manager).await
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
        Ok(item) => Json::<Item>(item).into_response(),
        Err(e) => {
            let (status, message) = match e {
                AppError::NotFound(_) => (
                    axum::http::StatusCode::NOT_FOUND,
                    "요청한 경매 아이템을 찾을 수 없습니다.",
                ),
                _ => (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "서버 내부 오류가 발생했습니다.",
                ),
            };

            (
                status,
                Json(serde_json::json!({
                    "code": match e {
                        AppError::NotFound(_) => "NOT_FOUND",
                        AppError::Database(_) => "DATABASE_ERROR",
                        _ => "INTERNAL_ERROR",
                    },
                    "message": message,
                    "details": e.to_string()
                })),
            )
                .into_response()
        }
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
        Ok(bid) => Json::<Option<i64>>(bid).into_response(),
        Err(e) => {
            let (status, message) = match e {
                AppError::NotFound(_) => (
                    axum::http::StatusCode::NOT_FOUND,
                    "요청한 경매 아이템을 찾을 수 없습니다.",
                ),
                _ => (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "서버 내부 오류가 발생했습니다.",
                ),
            };

            (
                status,
                Json(serde_json::json!({
                    "code": match e {
                        AppError::NotFound(_) => "NOT_FOUND",
                        AppError::Database(_) => "DATABASE_ERROR",
                        _ => "INTERNAL_ERROR",
                    },
                    "message": message,
                    "details": e.to_string()
                })),
            )
                .into_response()
        }
    }
}

/// 입찰 이력 조회
pub async fn handle_get_bid_history(
    State((db_manager, _)): State<(Arc<DatabaseManager>, Arc<KafkaProducer>)>,
    Path(item_id): Path<i64>,
) -> impl IntoResponse {
    info!("{:<12} --> 입찰 이력 조회 id: {}", "HandlerQuery", item_id);
    match query::handlers::get_bid_history(&db_manager, item_id).await {
        Ok(history) => Json::<Vec<Bid>>(history).into_response(),
        Err(e) => {
            let (status, message) = match e {
                AppError::NotFound(_) => (
                    axum::http::StatusCode::NOT_FOUND,
                    "요청한 경매 아이템을 찾을 수 없습니다.",
                ),
                _ => (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "서버 내부 오류가 발생했습니다.",
                ),
            };

            (
                status,
                Json(serde_json::json!({
                    "code": match e {
                        AppError::NotFound(_) => "NOT_FOUND",
                        AppError::Database(_) => "DATABASE_ERROR",
                        _ => "INTERNAL_ERROR",
                    },
                    "message": message,
                    "details": e.to_string()
                })),
            )
                .into_response()
        }
    }
}

/// 모든 상품 조회
pub async fn handle_get_items(
    State((db_manager, _)): State<(Arc<DatabaseManager>, Arc<KafkaProducer>)>,
) -> impl IntoResponse {
    info!("{:<12} --> 모든 상품 조회", "HandlerQuery");
    match query::handlers::get_all_items(&db_manager).await {
        Ok(items) => Json::<Vec<Item>>(items).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "code": match e {
                    AppError::Database(_) => "DATABASE_ERROR",
                    _ => "INTERNAL_ERROR",
                },
                "message": "서버 내부 오류가 발생했습니다.",
                "details": e.to_string()
            })),
        )
            .into_response(),
    }
}

/// 상품 조회
pub async fn handle_get_item(
    State((db_manager, _)): State<(Arc<DatabaseManager>, Arc<KafkaProducer>)>,
    Path(item_id): Path<i64>,
) -> impl IntoResponse {
    info!("{:<12} --> 상품 조회 id: {}", "HandlerQuery", item_id);
    match query::handlers::get_item(&db_manager, item_id).await {
        Ok(item) => Json::<Item>(item).into_response(),
        Err(e) => {
            let (status, message) = match e {
                AppError::NotFound(_) => (
                    axum::http::StatusCode::NOT_FOUND,
                    "요청한 경매 아이템을 찾을 수 없습니다.",
                ),
                _ => (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "서버 내부 오류가 발생했습니다.",
                ),
            };

            (
                status,
                Json(serde_json::json!({
                    "code": match e {
                        AppError::NotFound(_) => "NOT_FOUND",
                        AppError::Database(_) => "DATABASE_ERROR",
                        _ => "INTERNAL_ERROR",
                    },
                    "message": message,
                    "details": e.to_string()
                })),
            )
                .into_response()
        }
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
        Ok(bids) => Json::<Vec<Bid>>(bids).into_response(),
        Err(e) => {
            let (status, message) = match e {
                AppError::NotFound(_) => (
                    axum::http::StatusCode::NOT_FOUND,
                    "요청한 경매 아이템을 찾을 수 없습니다.",
                ),
                _ => (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "서버 내부 오류가 발생했습니다.",
                ),
            };

            (
                status,
                Json(serde_json::json!({
                    "code": match e {
                        AppError::NotFound(_) => "NOT_FOUND",
                        AppError::Database(_) => "DATABASE_ERROR",
                        _ => "INTERNAL_ERROR",
                    },
                    "message": message,
                    "details": e.to_string()
                })),
            )
                .into_response()
        }
    }
}

// endregion: --- Query Handlers
