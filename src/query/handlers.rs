// region:    --- Imports
use super::queries;
use crate::bidding::model::{Bid, Item};
use crate::database::DatabaseManager;
use crate::error::{AppError, Result};
use sqlx::Row;
use tracing::info;

// endregion: --- Imports

// region:    --- Query Handlers

/// 경매 상태 조회
pub async fn get_auction_state(db_manager: &DatabaseManager, item_id: i64) -> Result<Item> {
    info!("{:<12} --> 경매 상태 조회 id: {}", "Query", item_id);
    db_manager
        .transaction(|tx| {
            Box::pin(async move {
                let item = sqlx::query_as::<_, Item>(queries::GET_AUCTION_STATE)
                    .bind(item_id)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(AppError::from)?;
                Ok(item)
            })
        })
        .await
}

/// 최고 입찰가 조회
pub async fn get_highest_bid(db_manager: &DatabaseManager, item_id: i64) -> Result<Option<i64>> {
    info!("{:<12} --> 최고 입찰가 조회 id: {}", "Query", item_id);
    db_manager
        .transaction(|tx| {
            Box::pin(async move {
                let result = sqlx::query(queries::GET_HIGHEST_BID)
                    .bind(item_id)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(AppError::from)?;

                Ok(result.get("highest_bid"))
            })
        })
        .await
}

/// 입찰 이력 조회
pub async fn get_bid_history(db_manager: &DatabaseManager, item_id: i64) -> Result<Vec<Bid>> {
    info!("{:<12} --> 입찰 이력 조회 id: {}", "Query", item_id);
    db_manager
        .transaction(|tx| {
            Box::pin(async move {
                let bids = sqlx::query_as::<_, Bid>(queries::GET_BID_HISTORY)
                    .bind(item_id)
                    .fetch_all(&mut **tx)
                    .await
                    .map_err(AppError::from)?;
                Ok(bids)
            })
        })
        .await
}

/// 모든 상품 조회
pub async fn get_all_items(db_manager: &DatabaseManager) -> Result<Vec<Item>> {
    info!("{:<12} --> 모든 상품 조회", "Query");
    db_manager
        .transaction(|tx| {
            Box::pin(async move {
                let items = sqlx::query_as::<_, Item>(queries::GET_ALL_ITEMS)
                    .fetch_all(&mut **tx)
                    .await
                    .map_err(AppError::from)?;
                Ok(items)
            })
        })
        .await
}

/// 상품 조회
pub async fn get_item(db_manager: &DatabaseManager, item_id: i64) -> Result<Item> {
    info!("{:<12} --> 상품 조회 id: {}", "Query", item_id);
    db_manager
        .transaction(|tx| {
            Box::pin(async move {
                let item = sqlx::query_as::<_, Item>(queries::GET_ITEM)
                    .bind(item_id)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(AppError::from)?;
                Ok(item)
            })
        })
        .await
}

/// 상품 입찰 조회
pub async fn get_item_bids(db_manager: &DatabaseManager, item_id: i64) -> Result<Vec<Bid>> {
    info!("{:<12} --> 상품 입찰 조회 id: {}", "Query", item_id);
    db_manager
        .transaction(|tx| {
            Box::pin(async move {
                let bids = sqlx::query_as::<_, Bid>(queries::GET_ITEM_BIDS)
                    .bind(item_id)
                    .fetch_all(&mut **tx)
                    .await
                    .map_err(AppError::from)?;
                Ok(bids)
            })
        })
        .await
}

/// 상품 즉시 구매 가격 조회
pub async fn get_item_buy_now_price(db_manager: &DatabaseManager, item_id: i64) -> Result<i64> {
    info!(
        "{:<12} --> 상품 즉시 구매 가격 조회 id: {}",
        "Query", item_id
    );
    db_manager
        .transaction(|tx| {
            Box::pin(async move {
                let result = sqlx::query(queries::GET_ITEM_BUY_NOW_PRICE)
                    .bind(item_id)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(AppError::from)?;

                Ok(result.get("buy_now_price"))
            })
        })
        .await
}

/// 상품 버전 조회
pub async fn get_item_version(db_manager: &DatabaseManager, item_id: i64) -> Result<i64> {
    info!("{:<12} --> 상품 이벤트 버전 조회 id: {}", "Query", item_id);
    db_manager
        .transaction(|tx| {
            Box::pin(async move {
                let result = sqlx::query(queries::GET_ITEM_VERSION)
                    .bind(item_id)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(AppError::from)?;

                Ok(result.get("version"))
            })
        })
        .await
}

/// 상품 현재 가격 조회
pub async fn get_item_current_price(db_manager: &DatabaseManager, item_id: i64) -> Result<i64> {
    info!("{:<12} --> 상품 현재 가격 조회 id: {}", "Query", item_id);
    db_manager
        .transaction(|tx| {
            Box::pin(async move {
                let result = sqlx::query(queries::GET_ITEM_CURRENT_PRICE)
                    .bind(item_id)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(AppError::from)?;

                Ok(result.get("current_price"))
            })
        })
        .await
}

// endregion: --- Query Handlers
