/// 경매 상태 업데이트 스케줄러
/// 상품에 대한 상태를 관리하는 마이크로서비스는 별도로 있다 가정
/// 상품 관리 마이크로 서비스는 경매 시작 시간과 종료 시간에 따른 상태 업데이트를 한다고 가정
/// 다만 즉시 구매를 통해 낙찰이 되는 경우, 본 입찰 및 즉시구매 마이크로 서비스에서 완료 상태로 처리한다.
// region:    --- Imports
use chrono::Utc;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{debug, error};

// endregion: --- Imports

// region:    --- Auction Scheduler
/// 경매 상태 업데이트 스케줄러
pub struct AuctionScheduler {
    pool: Arc<PgPool>,
}

/// 경매 상태 업데이트 스케줄러 생성
impl AuctionScheduler {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// 경매 상태 업데이트 스케줄러 시작
    pub async fn start(&self) {
        let pool = Arc::clone(&self.pool);
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(1)); // 1초마다 실행
            loop {
                interval.tick().await;
                if let Err(e) = Self::update_auction_statuses(&pool).await {
                    error!(
                        "{:<12} --> 경매 상태 업데이트 중 오류 발생: {:?}",
                        "Scheduler", e
                    );
                }
            }
        });
    }

    /// 경매 상태 업데이트
    async fn update_auction_statuses(pool: &PgPool) -> Result<(), sqlx::Error> {
        let now = Utc::now();

        // SCHEDULED -> ACTIVE 상태 변경
        sqlx::query(
            "UPDATE items SET status = 'ACTIVE' 
             WHERE status = 'SCHEDULED' AND start_time <= $1",
        )
        .bind(now)
        .execute(pool)
        .await?;

        // ACTIVE -> COMPLETED 상태 변경
        sqlx::query(
            "UPDATE items SET status = 'COMPLETED' 
             WHERE status = 'ACTIVE' AND end_time <= $1",
        )
        .bind(now)
        .execute(pool)
        .await?;

        debug!(
            "{:<12} --> 경매 상태가 성공적으로 업데이트되었습니다.",
            "Scheduler"
        );

        Ok(())
    }
}
// endregion: --- Auction Scheduler
