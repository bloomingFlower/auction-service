use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum AuctionEvent {
    // 입찰 이벤트
    BidPlaced {
        item_id: i64,
        bidder_id: i64,
        bid_amount: i64,
        timestamp: DateTime<Utc>,
    },
    // 즉시 구매 이벤트
    BuyNowExecuted {
        item_id: i64,
        buyer_id: i64,
        price: i64,
        timestamp: DateTime<Utc>,
    },
}
