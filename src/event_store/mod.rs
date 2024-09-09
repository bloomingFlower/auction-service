// region:    --- Imports
use crate::auction::events::AuctionEvent;
use crate::message_broker::{KafkaConsumer, KafkaProducer};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use tracing::{error, info, warn};

// endregion: --- Imports

// region:    --- Event Model
/// 이벤트 저장소에 저장되는 이벤트 모델
#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct Event {
    pub id: i64,
    pub aggregate_id: i64,
    pub event_type: String,
    pub data: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub version: i64,
}
// endregion: --- Event Model

// region:    --- Event Store Trait
/// 이벤트 저장소 트레이트
#[async_trait]
pub trait EventStore {
    async fn append_and_publish_event(&self, event: Event) -> Result<(), String>;
}

/// 이벤트 저장소 구현체
pub struct PostgresEventStore {
    pool: Arc<PgPool>,
    kafka_producer: Arc<KafkaProducer>,
}

/// 이벤트 저장소 구현체 메서드 구현
#[async_trait]
impl EventStore for PostgresEventStore {
    async fn append_and_publish_event(&self, event: Event) -> Result<(), String> {
        let event_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO events (aggregate_id, event_type, data, timestamp, version)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (aggregate_id, version) DO NOTHING
            RETURNING id",
        )
        .bind(event.aggregate_id)
        .bind(&event.event_type)
        .bind(&event.data)
        .bind(event.timestamp)
        .bind(event.version)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "버전 충돌".to_string())?;

        // 이벤트를 카프카에 발행
        self.kafka_producer
            .send_message(
                "events",
                &event_id.to_string(),
                &serde_json::to_string(&event).unwrap(),
            )
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }
}

/// 이벤트 저장소 생성
impl PostgresEventStore {
    pub fn new(pool: Arc<PgPool>, kafka_producer: Arc<KafkaProducer>) -> Self {
        Self {
            pool,
            kafka_producer,
        }
    }
}

// endregion: --- Event Store

// region:    --- Event Consumer
/// 이벤트 소싱 구현체
pub struct EventConsumer {
    pool: Arc<PgPool>,
    kafka_consumer: Arc<KafkaConsumer>,
}

/// 이벤트 소싱 구현체 메서드 구현
impl EventConsumer {
    /// 이벤트 소싱 생성
    pub fn new(pool: Arc<PgPool>, kafka_consumer: Arc<KafkaConsumer>) -> Self {
        EventConsumer {
            pool,
            kafka_consumer,
        }
    }

    /// 이벤트 소싱 시작
    pub async fn start(&self) {
        let pool = Arc::clone(&self.pool);
        if let Err(e) = self
            .kafka_consumer
            .consume_events("events", move |event| {
                let pool = Arc::clone(&pool);
                // Return a boxed future
                Box::pin(async move {
                    if let Err(e) = Self::process_event(&pool, event).await {
                        error!("{:<12} --> 이벤트 처리 오류: {:?}", "EventConsume", e);
                    }
                    Ok(())
                })
            })
            .await
        {
            error!("{:<12} --> 이벤트 소비 오류: {:?}", "EventConsume", e);
        }
    }

    /// 이벤트 처리
    async fn process_event(pool: &PgPool, event: Event) -> Result<(), Box<dyn std::error::Error>> {
        match event.event_type.as_str() {
            "BidPlaced" => Self::handle_bid_placed(pool, &event).await?,
            "BuyNowExecuted" => Self::handle_buy_now_executed(pool, &event).await?,
            _ => warn!(
                "{:<12} --> 알 수 없는 이벤트 타입: {}",
                "EventConsume", event.event_type
            ),
        }
        Ok(())
    }

    /// 입찰 이벤트 처리
    async fn handle_bid_placed(pool: &PgPool, event: &Event) -> Result<(), sqlx::Error> {
        info!("{:<12} --> 입찰(BidPlaced)", "EventConsume");
        let bid_event: AuctionEvent = serde_json::from_value(event.data.clone())
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        if let AuctionEvent::BidPlaced {
            item_id,
            bidder_id,
            bid_amount,
            timestamp,
        } = bid_event
        {
            // 트랜잭션 시작
            let mut tx = pool.begin().await?;

            // 현재 가격 확인 및 업데이트
            let result = sqlx::query!(
                "UPDATE items SET current_price = $1 WHERE id = $2 AND current_price < $1 RETURNING current_price",
                bid_amount,
                item_id
            )
            .fetch_optional(&mut *tx)
            .await?;

            if let Some(row) = result {
                // 입찰 기록 추가
                sqlx::query!(
                    "INSERT INTO bids (item_id, bidder_id, bid_amount, bid_time) VALUES ($1, $2, $3, $4)",
                    item_id,
                    bidder_id,
                    bid_amount,
                    timestamp
                )
                .execute(&mut *tx)
                .await?;

                // 트랜잭션 커밋
                tx.commit().await?;
                info!(
                    "{:<12} --> 입찰 성공: 현재 가격 {}",
                    "EventConsume", row.current_price
                );
            } else {
                // 롤백
                tx.rollback().await?;
                info!(
                    "{:<12} --> 입찰 실패: 현재 가격이 더 높거나 같음",
                    "EventConsume"
                );
            }
        }
        Ok(())
    }

    /// 즉시 구매 이벤트 처리
    async fn handle_buy_now_executed(pool: &PgPool, event: &Event) -> Result<(), sqlx::Error> {
        info!("{:<12} --> 즉시 구매(BuyNowExecuted)", "EventConsume");
        let buy_now_event: AuctionEvent = serde_json::from_value(event.data.clone())
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        if let AuctionEvent::BuyNowExecuted {
            item_id,
            buyer_id,
            price,
            timestamp,
        } = buy_now_event
        {
            // 트랜잭션 시작
            let mut tx = pool.begin().await?;

            // 현재 가격 확인 및 상태 업데이트
            let result = sqlx::query!(
                "UPDATE items SET current_price = $1, status = 'COMPLETED' WHERE id = $2 AND current_price < $1 AND status != 'COMPLETED' RETURNING current_price",
                price,
                item_id
            )
            .fetch_optional(&mut *tx)
            .await?;

            if let Some(row) = result {
                // 즉시 구매 기록 추가
                sqlx::query!(
                    "INSERT INTO bids (item_id, bidder_id, bid_amount, bid_time) VALUES ($1, $2, $3, $4)",
                    item_id,
                    buyer_id,
                    price,
                    timestamp
                )
                .execute(&mut *tx)
                .await?;

                // 트랜잭션 커밋
                tx.commit().await?;
                info!(
                    "{:<12} --> 즉시 구매 성공: 최종 가격 {}",
                    "EventConsume", row.current_price
                );
            } else {
                // 롤백
                tx.rollback().await?;
                info!(
                    "{:<12} --> 즉시 구매 실패: 현재 가격이 더 높거나 같음, 또는 이미 완료된 경매",
                    "EventConsume"
                );
            }
        }
        Ok(())
    }
}
// endregion: --- Event Consumer
