/// 경매 상태 조회
pub const GET_AUCTION_STATE: &str = "SELECT id, title, description, starting_price, current_price, start_time, end_time, seller, status, created_at FROM items WHERE id = $1";

/// 최고 입찰 조회
pub const GET_HIGHEST_BID: &str =
    "SELECT MAX(bid_amount) as highest_bid FROM bids WHERE item_id = $1";

/// 입찰 이력 조회
pub const GET_BID_HISTORY: &str = r#"
    SELECT id, item_id, bidder_id, bid_amount, bid_time
    FROM bids
    WHERE item_id = $1
    ORDER BY bid_time DESC
"#;

/// 모든 상품 조회
pub const GET_ALL_ITEMS: &str = 
    "SELECT id, title, description, starting_price, current_price, buy_now_price, start_time, end_time, seller, status, created_at FROM items ORDER BY created_at DESC";

/// 상품 조회
pub const GET_ITEM: &str = 
    "SELECT id, title, description, starting_price, current_price, buy_now_price, start_time, end_time, seller, status, created_at FROM items WHERE id = $1";

/// 상품 입찰 조회
pub const GET_ITEM_BIDS: &str = r#"
    SELECT id, item_id, bidder_id, bid_amount, bid_time
    FROM bids
    WHERE item_id = $1
    ORDER BY bid_time DESC
"#;

/// 상품 현재 가격 조회
pub const GET_ITEM_CURRENT_PRICE: &str = "SELECT current_price FROM items WHERE id = $1";

/// 상품 즉시 구매 가격 조회
pub const GET_ITEM_BUY_NOW_PRICE: &str = "SELECT buy_now_price FROM items WHERE id = $1";

/// 상품 버전 조회
pub const GET_ITEM_VERSION: &str = "SELECT COALESCE(MAX(version), 0) as version FROM events WHERE aggregate_id = $1";