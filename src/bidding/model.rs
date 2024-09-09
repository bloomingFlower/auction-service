use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// 상품 모델
#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct Item {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub starting_price: i64,
    pub current_price: i64,
    pub buy_now_price: i64,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub seller: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

// 입찰 모델
#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct Bid {
    pub id: i64,
    pub item_id: i64,
    pub bidder_id: i64,
    pub bid_amount: i64,
    pub bid_time: DateTime<Utc>,
}
