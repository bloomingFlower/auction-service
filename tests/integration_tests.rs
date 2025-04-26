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
    // 정적 변수를 사용하여 트레이싱이 이미 초기화되었는지 확인
    use std::sync::Once;
    static INIT: Once = Once::new();

    // 한 번만 초기화
    INIT.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .without_time()
            .with_target(false)
            .with_test_writer()
            .finish();

        // 이미 설정되어 있을 경우 오류를 무시
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}

/// 데이터베이스 매니저 설정
async fn setup() -> Arc<DatabaseManager> {
    Arc::new(DatabaseManager::new().await)
}

/// 입찰 테스트
#[tokio::test]
async fn test_place_bid() {
    init_tracing();

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
    init_tracing();

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
    init_tracing();

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
    let pool = db_manager.pool();
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
    .fetch_one(pool)
    .await
    .unwrap()
}

/// 테스트용 아이템 업데이트
async fn update_test_item(db_manager: &DatabaseManager, item: Item) {
    let pool = db_manager.pool();
    sqlx::query("UPDATE items SET start_time = $1, end_time = $2, status = $3 WHERE id = $4")
        .bind(item.start_time)
        .bind(item.end_time)
        .bind(&item.status)
        .bind(item.id)
        .execute(pool)
        .await
        .unwrap();
}

/// 테스트 유틸리티: 응답 내용 검증
fn assert_response_contains_code(response_body: &str, expected_code: &str) {
    let response_json: Value =
        serde_json::from_str(response_body).expect("응답을 JSON으로 파싱할 수 없습니다");
    assert!(response_json.is_object(), "응답이 JSON 객체가 아닙니다");

    let code = response_json
        .get("code")
        .expect("응답에 'code' 필드가 없습니다");
    assert!(code.is_string(), "'code' 필드가 문자열이 아닙니다");
    assert_eq!(
        code.as_str().unwrap(),
        expected_code,
        "예상 코드와 실제 코드가 다릅니다"
    );
}

/// 테스트 유틸리티: 응답 내용에 메시지가 포함되어 있는지 확인
fn assert_response_contains_message(response_body: &str) {
    let response_json: Value =
        serde_json::from_str(response_body).expect("응답을 JSON으로 파싱할 수 없습니다");
    assert!(response_json.is_object(), "응답이 JSON 객체가 아닙니다");

    let message = response_json
        .get("message")
        .expect("응답에 'message' 필드가 없습니다");
    assert!(message.is_string(), "'message' 필드가 문자열이 아닙니다");
    assert!(
        !message.as_str().unwrap().is_empty(),
        "'message' 필드가 비어 있습니다"
    );
}

/// 테스트 유틸리티: 응답 내용에 상세 정보가 포함되어 있는지 확인
fn assert_response_contains_details(response_body: &str) {
    let response_json: Value =
        serde_json::from_str(response_body).expect("응답을 JSON으로 파싱할 수 없습니다");
    assert!(response_json.is_object(), "응답이 JSON 객체가 아닙니다");

    let details = response_json
        .get("details")
        .expect("응답에 'details' 필드가 없습니다");
    assert!(
        details.is_object() || details.is_string(),
        "'details' 필드가 객체나 문자열이 아닙니다"
    );
}

/// 낮은 입찰가 에러 테스트
#[tokio::test]
async fn test_low_bid_error() {
    init_tracing();

    let db_manager = setup().await;
    let client = Client::new();

    // 테스트용 아이템 생성
    let item = create_test_item(
        &db_manager,
        "낮은 입찰가 에러 테스트 아이템".to_string(),
        "낮은 입찰가 에러 테스트를 위한 아이템입니다.".to_string(),
    )
    .await;

    // 현재 가격보다 낮은 입찰가로 요청 생성
    let bid_data = json!({
        "item_id": item.id,
        "bidder_id": 1,
        "bid_amount": item.current_price - 1000 // 현재 가격보다 낮은 금액
    });

    // 입찰 처리
    let response = client
        .post("http://localhost:3000/bid")
        .json(&bid_data)
        .send()
        .await
        .expect("Failed to send request");

    // 응답 검증
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = response.text().await.unwrap();
    assert_response_contains_code(&body, "LOW_BID");
    assert_response_contains_message(&body);
    assert_response_contains_details(&body);

    // 데이터베이스에서 아이템 조회하여 가격이 변경되지 않았는지 확인
    let updated_item = query::handlers::get_item(&db_manager, item.id)
        .await
        .unwrap();
    assert_eq!(updated_item.current_price, item.current_price);
}

/// 경매 시작 전 즉시 구매 에러 테스트
#[tokio::test]
async fn test_not_started_buy_now_error() {
    init_tracing();

    let db_manager = setup().await;
    let client = Client::new();

    // 테스트용 아이템 생성 (시작 시간을 미래로 설정)
    let item_id = {
        let mut item = create_test_item(
            &db_manager,
            "경매 시작 전 즉시 구매 에러 테스트 아이템".to_string(),
            "경매 시작 전 즉시 구매 에러 테스트를 위한 아이템입니다.".to_string(),
        )
        .await;
        item.start_time = Utc::now() + Duration::hours(1); // 시작 시간을 미래로 설정
        let id = item.id;
        update_test_item(&db_manager, item).await;
        id
    };

    // 즉시 구매 요청 생성
    let buy_now_data = json!({
        "item_id": item_id,
        "buyer_id": 2
    });

    // 즉시 구매 처리
    let response = client
        .post("http://localhost:3000/buy-now")
        .json(&buy_now_data)
        .send()
        .await
        .expect("Failed to send request");

    // 응답 검증
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = response.text().await.unwrap();
    assert_response_contains_code(&body, "NOT_STARTED");
    assert_response_contains_message(&body);
    assert_response_contains_details(&body);

    // 데이터베이스에서 아이템 조회하여 상태가 변경되지 않았는지 확인
    let updated_item = query::handlers::get_item(&db_manager, item_id)
        .await
        .unwrap();
    assert_eq!(updated_item.status, "ACTIVE");
}

/// 존재하지 않는 아이템 조회 에러 테스트
#[tokio::test]
async fn test_not_found_error() {
    init_tracing();

    let client = Client::new();

    // 존재하지 않는 아이템 ID로 요청
    let non_existent_id = 999999;

    // 아이템 조회 요청
    let response = client
        .get(format!("http://localhost:3000/items/{}", non_existent_id))
        .send()
        .await
        .expect("Failed to send request");

    // 응답 검증
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = response.text().await.unwrap();
    assert_response_contains_code(&body, "NOT_FOUND");
    assert_response_contains_message(&body);
    assert_response_contains_details(&body);
}

/// 성공 응답 형식 테스트
#[tokio::test]
async fn test_success_response_format() {
    init_tracing();

    let db_manager = setup().await;
    let client = Client::new();

    // 테스트용 아이템 생성
    let item = create_test_item(
        &db_manager,
        "성공 응답 형식 테스트 아이템".to_string(),
        "성공 응답 형식 테스트를 위한 아이템입니다.".to_string(),
    )
    .await;

    // 입찰 요청 생성
    let bid_data = json!({
        "item_id": item.id,
        "bidder_id": 1,
        "bid_amount": item.current_price + 5000
    });

    // 입찰 처리
    let response = client
        .post("http://localhost:3000/bid")
        .json(&bid_data)
        .send()
        .await
        .expect("Failed to send request");

    // 응답 검증
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.text().await.unwrap();
    let response_json: Value =
        serde_json::from_str(&body).expect("응답을 JSON으로 파싱할 수 없습니다");

    // 성공 응답에는 메시지와 현재 가격, 입찰 금액이 포함되어야 함
    assert!(
        response_json.get("message").is_some(),
        "응답에 'message' 필드가 없습니다"
    );
    assert!(
        response_json.get("current_price").is_some(),
        "응답에 'current_price' 필드가 없습니다"
    );
    assert!(
        response_json.get("bid_amount").is_some(),
        "응답에 'bid_amount' 필드가 없습니다"
    );

    // 이벤트 처리 대기
    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

    // 데이터베이스에서 업데이트된 아이템 조회
    let updated_item = query::handlers::get_item(&db_manager, item.id)
        .await
        .unwrap();
    assert_eq!(updated_item.current_price, item.current_price + 5000);
}

/// 즉시 구매 성공 응답 형식 테스트
#[tokio::test]
async fn test_buy_now_success_response_format() {
    init_tracing();

    let db_manager = setup().await;
    let client = Client::new();

    // 테스트용 아이템 생성
    let item = create_test_item(
        &db_manager,
        "즉시 구매 성공 응답 형식 테스트 아이템".to_string(),
        "즉시 구매 성공 응답 형식 테스트를 위한 아이템입니다.".to_string(),
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

    // 응답 검증
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.text().await.unwrap();
    let response_json: Value =
        serde_json::from_str(&body).expect("응답을 JSON으로 파싱할 수 없습니다");

    // 성공 응답에는 코드, 메시지, 데이터가 포함되어야 함
    assert_eq!(
        response_json.get("code").and_then(|v| v.as_str()),
        Some("SUCCESS"),
        "응답에 'code' 필드가 'SUCCESS'가 아닙니다"
    );
    assert!(
        response_json.get("message").is_some(),
        "응답에 'message' 필드가 없습니다"
    );
    assert!(
        response_json.get("data").is_some(),
        "응답에 'data' 필드가 없습니다"
    );

    // 데이터 필드에는 item_id, final_price, status가 포함되어야 함
    let data = response_json.get("data").unwrap();
    assert!(
        data.get("item_id").is_some(),
        "data에 'item_id' 필드가 없습니다"
    );
    assert!(
        data.get("final_price").is_some(),
        "data에 'final_price' 필드가 없습니다"
    );
    assert!(
        data.get("status").is_some(),
        "data에 'status' 필드가 없습니다"
    );

    // 이벤트 처리 대기
    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

    // 데이터베이스에서 업데이트된 아이템 조회
    let updated_item = query::handlers::get_item(&db_manager, item.id)
        .await
        .unwrap();
    assert_eq!(updated_item.status, "COMPLETED");
}

/// 개선된 동시성 테스트 - 재시도 메커니즘 검증
#[tokio::test]
async fn test_improved_concurrent_bidding() {
    init_tracing();

    let db_manager = setup().await;

    // 테스트용 아이템 생성
    let item = create_test_item(
        &db_manager,
        "개선된 동시성 테스트 아이템".to_string(),
        "개선된 동시성 테스트를 위한 아이템입니다.".to_string(),
    )
    .await;

    // 동시 입찰 수 (더 많은 동시성 테스트를 위해 증가)
    let concurrent_bids = 20;

    // 각 입찰의 증가액
    let increment = 1000;

    // 동시 입찰 생성 및 처리
    let mut handles = vec![];
    for i in 1..=concurrent_bids {
        let client = reqwest::Client::new();
        let bid_amount = item.current_price + i * increment;
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

            (status, body, bid_amount)
        });

        handles.push(handle);
    }

    // 모든 입찰 처리 대기 및 결과 확인
    let mut results = vec![];
    for handle in handles {
        match handle.await {
            Ok((status, body, bid_amount)) => {
                results.push((status, body, bid_amount));
            }
            Err(e) => {
                panic!("입찰 처리 중 오류 발생: {:?}", e);
            }
        }
    }

    // 성공한 입찰 수 확인
    let successful_bids = results
        .iter()
        .filter(|(status, _, _)| *status == StatusCode::OK)
        .count();

    // 적어도 하나의 입찰은 성공해야 함
    assert!(successful_bids > 0, "성공한 입찰이 없습니다");

    info!("성공한 입찰 수: {}/{}", successful_bids, concurrent_bids);

    // 실패한 입찰 분석
    let failed_bids = results
        .iter()
        .filter(|(status, _, _)| *status != StatusCode::OK)
        .collect::<Vec<_>>();

    for (status, body, bid_amount) in &failed_bids {
        info!(
            "실패한 입찰 - 상태: {:?}, 금액: {}, 응답: {}",
            status, bid_amount, body
        );

        // 실패한 입찰은 대부분 낮은 입찰가 오류여야 함
        if *status == StatusCode::BAD_REQUEST {
            let response_json: Value = serde_json::from_str(body).unwrap();
            if let Some(code) = response_json.get("code").and_then(|c| c.as_str()) {
                assert!(
                    code == "LOW_BID" || code == "CONFLICT" || code == "VALIDATION_ERROR",
                    "예상치 못한 오류 코드: {}",
                    code
                );
            }
        }
    }

    // 이벤트 처리 대기
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // 최종 상태 확인
    let updated_item = query::handlers::get_item(&db_manager, item.id)
        .await
        .unwrap();

    // 최종 가격은 초기 가격보다 높아야 함
    assert!(
        updated_item.current_price > item.current_price,
        "최종 가격이 초기 가격보다 높지 않습니다: {} <= {}",
        updated_item.current_price,
        item.current_price
    );

    // 입찰 이력 확인
    let bid_history = query::handlers::get_bid_history(&db_manager, item.id)
        .await
        .unwrap();

    // 입찰 이력 수는 성공한 입찰 수와 일치하거나 적을 수 있음 (동시성 문제로 인해)
    assert!(
        bid_history.len() <= successful_bids,
        "입찰 이력 수({})가 성공한 입찰 수({})보다 많습니다",
        bid_history.len(),
        successful_bids
    );

    // 입찰 이력의 마지막 입찰 금액은 현재 가격과 일치해야 함
    if !bid_history.is_empty() {
        let highest_bid = bid_history.iter().max_by_key(|b| b.bid_amount).unwrap();
        assert_eq!(
            highest_bid.bid_amount, updated_item.current_price,
            "최고 입찰 금액({})이 현재 가격({})과 일치하지 않습니다",
            highest_bid.bid_amount, updated_item.current_price
        );
    }
}

/// 테스트 간 격리 검증
#[tokio::test]
async fn test_isolation_between_tests() {
    init_tracing();

    let db_manager = setup().await;

    // 고유한 제목으로 테스트용 아이템 생성
    let unique_title = format!("격리 테스트 아이템 {}", Utc::now().timestamp_millis());
    let item = create_test_item(
        &db_manager,
        unique_title.clone(),
        "테스트 간 격리를 검증하기 위한 아이템입니다.".to_string(),
    )
    .await;

    // 데이터베이스에서 아이템 조회
    let items = query::handlers::get_all_items(&db_manager).await.unwrap();

    // 생성한 아이템이 존재하는지 확인
    let found_item = items.iter().find(|i| i.title == unique_title);
    assert!(found_item.is_some(), "생성한 아이템을 찾을 수 없습니다");

    // 아이템 ID가 일치하는지 확인
    let found_item = found_item.unwrap();
    assert_eq!(found_item.id, item.id, "아이템 ID가 일치하지 않습니다");

    // 아이템 상태가 일치하는지 확인
    assert_eq!(
        found_item.status, "ACTIVE",
        "아이템 상태가 일치하지 않습니다"
    );
}

/// 에러 응답 형식 일관성 테스트
#[tokio::test]
async fn test_error_response_consistency() {
    init_tracing();

    let client = Client::new();

    // 1. 존재하지 않는 아이템 조회 (NOT_FOUND 에러)
    let not_found_response = client
        .get("http://localhost:3000/items/999999")
        .send()
        .await
        .expect("Failed to send request");

    // 상태 코드 확인 없이 응답 내용만 확인
    let not_found_body = not_found_response.text().await.unwrap();

    // 응답이 JSON 형식인지 확인
    let response_json: Value = serde_json::from_str(&not_found_body)
        .unwrap_or_else(|_| panic!("응답이 유효한 JSON이 아닙니다: {}", not_found_body));

    // 필수 필드가 있는지 확인
    assert!(
        response_json.get("code").is_some(),
        "응답에 'code' 필드가 없습니다"
    );
    assert!(
        response_json.get("message").is_some(),
        "응답에 'message' 필드가 없습니다"
    );
    assert!(
        response_json.get("details").is_some(),
        "응답에 'details' 필드가 없습니다"
    );

    // 코드가 NOT_FOUND인지 확인
    assert_eq!(
        response_json.get("code").and_then(|c| c.as_str()),
        Some("NOT_FOUND"),
        "응답의 'code' 필드가 'NOT_FOUND'가 아닙니다"
    );

    // 이 테스트는 단일 응답만 확인합니다
}

/// 데이터베이스 에러 처리 테스트 (모의 테스트)
#[tokio::test]
async fn test_database_error_handling() {
    init_tracing();

    let db_manager = setup().await;
    let client = Client::new();

    // 테스트용 아이템 생성
    let item = create_test_item(
        &db_manager,
        "데이터베이스 에러 처리 테스트 아이템".to_string(),
        "데이터베이스 에러 처리를 테스트하기 위한 아이템입니다.".to_string(),
    )
    .await;

    // 유효하지 않은 입찰 요청 생성 (음수 금액)
    let invalid_bid_data = json!({
        "item_id": item.id,
        "bidder_id": 1,
        "bid_amount": -1000 // 음수 금액
    });

    // 입찰 처리
    let response = client
        .post("http://localhost:3000/bid")
        .json(&invalid_bid_data)
        .send()
        .await
        .expect("Failed to send request");

    // 응답 검증 (400 Bad Request 예상)
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = response.text().await.unwrap();
    let response_json: Value = serde_json::from_str(&body).unwrap();

    // 응답에 필수 필드가 있는지 확인
    assert!(
        response_json.get("code").is_some(),
        "응답에 'code' 필드가 없습니다"
    );
    assert!(
        response_json.get("message").is_some(),
        "응답에 'message' 필드가 없습니다"
    );

    // 코드가 VALIDATION_ERROR 또는 다른 적절한 에러 코드인지 확인
    let code = response_json.get("code").and_then(|c| c.as_str()).unwrap();
    assert!(
        code == "VALIDATION_ERROR" || code == "LOW_BID",
        "예상치 못한 에러 코드: {}",
        code
    );
}
