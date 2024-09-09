/// 입찰 관련 커맨드 처리
/// 1. 입찰
/// 2. 즉시 구매
// region:    --- Imports
use crate::auction::events::AuctionEvent;
use crate::database::DatabaseManager;
use crate::event_store::{Event, EventStore};
use crate::query::handlers;
use crate::query::handlers::get_item_version;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
// endregion: --- Imports

// region:    --- Commands
/// 입찰 명령
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlaceBidCommand {
    pub item_id: i64,
    pub bidder_id: i64,
    pub bid_amount: i64,
}

/// 즉시 구매 명령
#[derive(Debug, Serialize, Deserialize)]
pub struct BuyNowCommand {
    pub item_id: i64,
    pub buyer_id: i64,
}

// 최대 재시도 횟수
const MAX_RETRIES: i32 = 100;

/// 1. 입찰
pub async fn handle_place_bid(
    cmd: PlaceBidCommand,
    event_store: &impl EventStore,
    db_manager: &DatabaseManager,
) -> Result<(), serde_json::Value> {
    info!("{:<12} --> 입찰 요청 처리 시작: {:?}", "Command", cmd);
    let mut retries = 0;

    while retries < MAX_RETRIES {
        // 현재 버전 조회
        let current_version = get_item_version(db_manager, cmd.item_id)
            .await
            .map_err(|e| serde_json::json!({"error": e.to_string()}))?;

        // 아이템 정보 조회
        let item = handlers::get_item(db_manager, cmd.item_id)
            .await
            .map_err(|e| serde_json::json!({"error": e.to_string()}))?;

        let now = Utc::now();

        // 경매 상태 및 시간 검증
        if now < item.start_time {
            return Err(serde_json::json!({
                "error": "경매가 아직 시작되지 않았습니다.",
                "code": "NOT_STARTED"
            }));
        }

        match item.status.as_str() {
            "SCHEDULED" => {
                return Err(
                    serde_json::json!({"error": "경매가 아직 시작되지 않았습니다.", "code": "NOT_STARTED"}),
                )
            }
            "COMPLETED" => {
                return Err(
                    serde_json::json!({"error": "경매가 이미 종료되었습니다.", "code": "ALREADY_ENDED"}),
                )
            }
            _ if now > item.end_time => {
                return Err(
                    serde_json::json!({"error": "경매가 이미 종료되었습니다.", "code": "ALREADY_ENDED"}),
                )
            }
            "ACTIVE" if now <= item.end_time => {
                if cmd.bid_amount <= item.current_price {
                    return Err(serde_json::json!({
                        "error": "입찰 금액이 현재 가격보다 낮습니다.",
                        "code": "LOW_BID",
                        "bid_amount": cmd.bid_amount,
                    }));
                }

                // 입찰 금액이 즉시구매 가격 이상인 경우 낙찰 처리
                if cmd.bid_amount >= item.buy_now_price {
                    let buy_now_event = AuctionEvent::BuyNowExecuted {
                        item_id: cmd.item_id,
                        buyer_id: cmd.bidder_id,
                        price: item.buy_now_price, // 입찰가 대신 즉시구매 가격으로 처리
                        timestamp: now,
                    };

                    let event = Event {
                        id: 0,
                        aggregate_id: cmd.item_id,
                        event_type: "BuyNowExecuted".to_string(),
                        data: serde_json::to_value(buy_now_event)
                            .map_err(|e| serde_json::json!({"error": e.to_string()}))?,
                        timestamp: now,
                        version: current_version + 1,
                    };

                    // 이벤트 저장 및 발행
                    match event_store.append_and_publish_event(event).await {
                        Ok(_) => {
                            info!(
                                "{:<12} --> BuyNowExecuted 이벤트가 성공적으로 저장되었습니다.",
                                "Command"
                            );
                            return Ok(());
                        }
                        Err(e) if e.contains("버전 충돌") => {
                            retries += 1;
                            continue;
                        }
                        Err(e) => return Err(serde_json::json!({"error": e})),
                    }
                }

                // 입찰 이벤트 생성
                let bid_event = AuctionEvent::BidPlaced {
                    item_id: cmd.item_id,
                    bidder_id: cmd.bidder_id,
                    bid_amount: cmd.bid_amount,
                    timestamp: now,
                };

                let event = Event {
                    id: 0,
                    aggregate_id: cmd.item_id,
                    event_type: "BidPlaced".to_string(),
                    data: serde_json::to_value(bid_event)
                        .map_err(|e| serde_json::json!({"error": e.to_string()}))?,
                    timestamp: now,
                    version: current_version + 1,
                };

                // 이벤트 저장 및 발행
                match event_store.append_and_publish_event(event).await {
                    Ok(_) => return Ok(()),
                    Err(e) if e.contains("버전 충돌") => {
                        warn!(
                            "{:<12} --> 낙관적 업데이트로 인한 버전 충돌: 재시도",
                            "Command"
                        );
                        retries += 1;
                        continue;
                    }
                    Err(e) => return Err(serde_json::json!({"error": e})),
                }
            }
            _ => {
                return Err(
                    serde_json::json!({"error": "잘못된 경매 상태입니다.", "code": "INVALID_STATUS"}),
                )
            }
        }
    }

    Err(serde_json::json!({"error": "최대 재시도 횟수 초과", "code": "MAX_RETRIES_EXCEEDED"}))
}

/// 2. 즉시 구매(낙찰)
pub async fn handle_buy_now(
    cmd: BuyNowCommand,
    buy_now_price: i64,
    event_store: &impl EventStore,
    db_manager: &DatabaseManager,
) -> Result<(), serde_json::Value> {
    info!("{:<12} --> 즉시 구매 요청 처리 시작: {:?}", "Command", cmd);
    let mut retries = 0;

    while retries < MAX_RETRIES {
        // 현재 버전 조회
        let current_version = get_item_version(db_manager, cmd.item_id)
            .await
            .map_err(|e| serde_json::json!({"error": e.to_string()}))?;

        // 아이템 정보 조회
        let item = handlers::get_item(db_manager, cmd.item_id)
            .await
            .map_err(|e| serde_json::json!({"error": e.to_string()}))?;

        let now = Utc::now();

        // 경매 상태 및 시간 검증
        if now < item.start_time {
            return Err(serde_json::json!({
                "error": "경매가 아직 시작되지 않았습니다.",
                "code": "NOT_STARTED"
            }));
        }

        match item.status.as_str() {
            "SCHEDULED" => {
                return Err(
                    serde_json::json!({"error": "경매가 아직 시작되지 않았습니다.", "code": "NOT_STARTED"}),
                )
            }
            "COMPLETED" => {
                return Err(
                    serde_json::json!({"error": "경매가 이미 종료되었습니다.", "code": "ALREADY_ENDED"}),
                )
            }
            _ if now > item.end_time => {
                return Err(
                    serde_json::json!({"error": "경매가 이미 종료되었습니다.", "code": "ALREADY_ENDED"}),
                )
            }
            "ACTIVE" if now <= item.end_time => {
                // 즉시 구매 이벤트 생성
                let buy_now_event = AuctionEvent::BuyNowExecuted {
                    item_id: cmd.item_id,
                    buyer_id: cmd.buyer_id,
                    price: buy_now_price,
                    timestamp: now,
                };

                let event = Event {
                    id: 0,
                    aggregate_id: cmd.item_id,
                    event_type: "BuyNowExecuted".to_string(),
                    data: serde_json::to_value(buy_now_event)
                        .map_err(|e| serde_json::json!({"error": e.to_string()}))?,
                    timestamp: now,
                    version: current_version + 1, // 현재 이벤트 버전 + 1
                };

                // 이벤트 저장 및 발행
                match event_store.append_and_publish_event(event).await {
                    Ok(_) => {
                        info!(
                            "{:<12} --> BuyNowExecuted 이벤트가 성공적으로 저장되었습니다.",
                            "Command"
                        );
                        return Ok(());
                    }
                    Err(e) if e.contains("버전 충돌") => {
                        retries += 1;
                        continue;
                    }
                    Err(e) => return Err(serde_json::json!({"error": e})),
                }
            }
            _ => {
                return Err(
                    serde_json::json!({"error": "잘못된 경매 상태입니다.", "code": "INVALID_STATUS"}),
                )
            }
        }
    }

    Err(serde_json::json!({"error": "최대 재시도 횟수 초과", "code": "MAX_RETRIES_EXCEEDED"}))
}

// endregion: --- Commands
