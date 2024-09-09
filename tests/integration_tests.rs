use auction_service::bidding::model::Item;
use auction_service::database::DatabaseManager;
use auction_service::query;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use reqwest::Client;
use serde_json::json;
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info};

/// 트레이싱 초기화
fn init_tracing() {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .without_time()
        .with_target(false)
        .with_test_writer()
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("트레이싱 구독자 설정 실패");
}

/// 데이터베이스 매니저 설정
async fn setup() -> Arc<DatabaseManager> {
    Arc::new(DatabaseManager::new().await)
}

/// 입찰 테스트
#[tokio::test]
async fn test_place_bid() {
    let db_manager = setup().await;
    let client = Client::new();

    // 테스트용 아이템 생성
    let item = create_test_item(
        &db_manager,
        "입찰 테스트 아이템".to_string(),
        "입찰 기능 테스트를 위한 아이템입니다.".to_string(),
    )
    .await;

    // 입찰 요청 생성
    let bid_data = json!({
        "item_id": item.id,
        "bidder_id": 1,
        "bid_amount": item.current_price + 1000
    });

    // 입찰 처리
    let response = client
        .post("http://localhost:3000/bid")
        .json(&bid_data)
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success());

    // 이벤트 처리 대기
    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

    // 데이터베이스에서 업데이트된 아이템 조회
    let updated_item = query::handlers::get_item(&db_manager, item.id)
        .await
        .unwrap();
    assert_eq!(updated_item.current_price, item.current_price + 1000);
}

/// 즉시 구매 테스트
#[tokio::test]
async fn test_buy_now() {
    let db_manager = setup().await;
    let client = Client::new();

    // 테스트용 아이템 생성
    let item = create_test_item(
        &db_manager,
        "즉시 구매 테스트 아이템".to_string(),
        "즉시 구매 기능 테스트를 위한 아이템입니다.".to_string(),
    )
    .await;

    // 즉시 구매 요청 생성
    let buy_now_data = json!({
        "item_id": item.id,
        "buyer_id": 2
    });

    // 즉시 구매 처리
    let response = client
        .post("http://localhost:3000/buy-now")
        .json(&buy_now_data)
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success());

    // 이벤트 처리 대기
    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

    // 데이터베이스에서 업데이트된 아이템 조회
    let updated_item = query::handlers::get_item(&db_manager, item.id)
        .await
        .unwrap();
    assert_eq!(updated_item.status, "COMPLETED");
}

/// 경매 사이클 테스트
#[tokio::test]
async fn test_auction_lifecycle() {
    let db_manager = setup().await;
    let client = Client::new();

    // 테스트용 아이템 생성 (시작 시간을 현재 시간으로 설정)
    let item_id = {
        let mut item = create_test_item(
            &db_manager,
            "경매 사이클 테스트 아이템".to_string(),
            "경매 사이클 테스트(입찰 및 종료 대기, 종료 후 상태 확인)를 위한 아이템입니다."
                .to_string(),
        )
        .await;
        item.start_time = Utc::now();
        item.end_time = Utc::now() + Duration::seconds(5);
        let id = item.id;
        update_test_item(&db_manager, item).await;
        id
    };

    // 경매 시작 전 상태 확인
    let initial_item = query::handlers::get_item(&db_manager, item_id)
        .await
        .unwrap();
    assert_eq!(initial_item.status, "ACTIVE");

    // 입찰 요청 생성
    let bid_data = json!({
        "item_id": item_id,
        "bidder_id": 1,
        "bid_amount": initial_item.current_price + 5000
    });

    // 입찰 처리
    let response = client
        .post("http://localhost:3000/bid")
        .json(&bid_data)
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success());

    // 이벤트 처리 대기
    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

    // 현재 상태 확인
    let current_item = query::handlers::get_item(&db_manager, item_id)
        .await
        .unwrap();
    assert_eq!(
        current_item.current_price,
        initial_item.current_price + 5000
    );

    // 경매 종료 대기
    tokio::time::sleep(tokio::time::Duration::from_secs(6)).await;

    // 경매 종료 후 상태 확인
    let final_item = query::handlers::get_item(&db_manager, item_id)
        .await
        .unwrap();
    assert_eq!(final_item.status, "COMPLETED");
}

/// 동시성 입찰 테스트
#[tokio::test]
async fn test_concurrent_bidding() {
    // 테스트 시작 시 tracing 초기화
    init_tracing();

    let db_manager = setup().await;

    // 3개의 테스트용 아이템 생성
    let items = create_multiple_test_items(&db_manager, 3).await;

    // 각 아이템에 대해 동시 입찰 생성 및 처리
    for (index, item) in items.iter().enumerate() {
        info!("아이템 {} 테스트 시작", index + 1);

        // 50개의 동시 입찰 생성
        let mut handles = vec![];
        for i in 1..=50 {
            let client = reqwest::Client::new();
            let bid_amount = item.current_price + i * 1000;
            let item_id = item.id;

            let handle = tokio::spawn(async move {
                let bid_data = serde_json::json!({
                    "item_id": item_id,
                    "bidder_id": i,
                    "bid_amount": bid_amount
                });

                // POST 요청 전송
                let response = client
                    .post(format!("http://{}/bid", "127.0.0.1:3000"))
                    .header("Content-Type", "application/json")
                    .json(&bid_data)
                    .send()
                    .await
                    .unwrap();

                let status = response.status();
                let body = response.text().await.unwrap();

                (status, body)
            });

            handles.push(handle);
        }

        // 모든 입찰 처리 대기 및 결과 확인
        let mut successful_bids = 0;
        let mut failed_bids = 0;
        for handle in handles {
            let (status, body) = handle.await.unwrap();

            if status == StatusCode::OK {
                successful_bids += 1;
            } else if status == StatusCode::BAD_REQUEST {
                let error_info: Value = serde_json::from_str(&body).unwrap();
                if error_info["code"] == "MAX_RETRIES_EXCEEDED" {
                    error!("최대 재시도 횟수 초과 오류 발생: {:?}", error_info);
                    panic!("최대 재시도 횟수 초과 오류 발생");
                } else {
                    failed_bids += 1;
                }
            }
        }

        info!(
            "아이템 {}: 성공한 입찰 수: {}, 실패한 입찰 수: {}",
            index + 1,
            successful_bids,
            failed_bids
        );

        // 이벤트 처리 대기
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // 최종 상태 확인
        let updated_item = query::handlers::get_item(&db_manager, item.id)
            .await
            .unwrap();
        assert_eq!(
            updated_item.current_price,
            item.current_price + 50000,
            "아이템 {}: 예상 가격: {}, 실제 가격: {}",
            index + 1,
            item.current_price + 50000,
            updated_item.current_price
        );

        // 입찰 이력 확인
        let bid_history = query::handlers::get_bid_history(&db_manager, item.id)
            .await
            .unwrap();
        info!("아이템 {}: 총 입찰 수: {}", index + 1, bid_history.len());

        // 버전 확인
        let final_version = query::handlers::get_item_version(&db_manager, item.id)
            .await
            .unwrap();
        assert!(final_version >= 1);
    }
}

// 여러 개의 테스트 아이템을 생성하는 함수
async fn create_multiple_test_items(db_manager: &DatabaseManager, count: usize) -> Vec<Item> {
    let mut items = Vec::with_capacity(count);
    for i in 1..=count {
        let item = create_test_item(
            db_manager,
            format!("동시성 입찰 테스트 아이템 {}", i),
            format!("동시성 입찰 기능 테스트를 위한 아이템 {}입니다.", i),
        )
        .await;
        items.push(item);
    }
    items
}

/// 테스트용 아이템 생성
async fn create_test_item(
    db_manager: &DatabaseManager,
    title: String,
    description: String,
) -> Item {
    db_manager.transaction(|tx| Box::pin(async move {
        sqlx::query_as::<_, Item>(
            "INSERT INTO items (title, description, starting_price, current_price, buy_now_price, start_time, end_time, seller, status, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             RETURNING *"
        )
        .bind(&title)
        .bind(&description)
        .bind(10000)
        .bind(10000)
        .bind(500000)
        .bind(Utc::now())
        .bind(Utc::now() + Duration::hours(2))
        .bind("TestSeller")
        .bind("ACTIVE")
        .bind(Utc::now())
        .fetch_one(&mut **tx)
        .await
    })).await.unwrap()
}

/// 테스트용 아이템 업데이트
async fn update_test_item(db_manager: &DatabaseManager, item: Item) {
    db_manager
        .transaction(|tx| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE items SET start_time = $1, end_time = $2, status = $3 WHERE id = $4",
                )
                .bind(item.start_time)
                .bind(item.end_time)
                .bind(&item.status)
                .bind(item.id)
                .execute(&mut **tx)
                .await
            })
        })
        .await
        .unwrap();
}
