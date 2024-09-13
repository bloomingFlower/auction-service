# Online Auction System

## 개요

입찰 관리, 경매 상태 조회, 그리고 즉시 구매 기능을 담당하는 마이크로 서비스 구현.

- Event-Driven-Architecture
- Scale-out
- 경매 시작 및 종료는 자동으로 처리
- 입찰 및 즉시 구매 처리 시 데이터 일관성 유지
- 상품이나 결제 등을 처리하는 다른 마이크로 서비스들이 있다고 가정

## 적용 기술

- Rust axum: 웹 서버
- PostgreSQL: 상품 정보, 입찰 내역, 이벤트 정보 저장
- Kafka: 이벤트 저장소, 스케일 아웃, 메시지 큐

## 구현 개요

- Event Sourcing 패턴 적용
- CQRS 패턴 적용

  - Command
    - 입찰 추가: 입찰은 최고 입찰가 및 상품 경매 상태를 확인하여 처리. 입찰가가 즉시 구매가보다 높을 경우 즉시 구매가로 즉시 구매 처리.
    - 즉시 구매: 즉시 구매 시 상품 상태를 완료 상태로 변경.
  - Query
    - 실시간 입찰 목록: 상품별 입찰 목록을 실시간으로 확인 가능
    - 최고 입찰가 확인: 상품별 최고 입찰가를 실시간으로 확인 가능
    - 실시간 경매 상태 확인: 상품별 경매 상태를 실시간으로 확인 가능 (시작 예정, 진행 중, 완료)

## 가정 사항

- 상품 관리, 결제 등의 다른 마이크로서비스는 구현되어 있다고 가정합니다.
- command 로 시간에 따른 상품의 경매 상태 업데이트를 명시하고 있지 않으므로 가상의 상품 상태 이벤트를 소싱하는 마이크로 서비스가 별도로 있다고 가정하고 `scheduler` 서비스가 1초마다 상태를 업데이트 합니다.

## 프로젝트 구조

```text
src/
├── auction: 경매 상태 관리
├── bidding: 입찰 관리(command)
├── database: 데이터베이스 정의
├── event_store: 이벤트 저장소(event-sourcing)
├── message_queue: 메시지 큐 스트림(Kafka)
├── query: 쿼리 서비스(실시간 입찰 목록, 최고 입찰가 확인, 실시간 경매 상태 확인)
├── scheduler: 상품 상태 관리를 위한 스케줄러
├── sql: 쿼리 서비스를 위한 쿼리 정의
└── tests: 통합 테스트
```

## 프로젝트 실행

PostgreSQL, Kafka, Zookeeper를 먼저 실행합니다.

```bash
cd docker
docker compose up -d
```

docker 서비스 동작 확인 후, 프로젝트를 실행합니다.
`.cargo/config.toml` 파일에서 `RUST_LOG` 값을 변경하여 상세한 동작을 확인할 수 있습니다. (e.g., `info`)

```bash
# 실행(릴리즈 모드), 필요 시, 병렬 실행 (-j 8)
cargo run --release
```

## 프로젝트 테스트

앞서 프로젝트 실행을 확인 후, 테스트를 수행합니다.

```bash
# 테스트 전 cargo run --release 실행
cargo test --release --test integration_tests
```

테스트 케이스는 총 4가지 입니다.

- 입찰 테스트
- 즉시 구매 테스트
- 경매 사이클 테스트(입찰 및 시간 경과에 따른 경매 상태 변경)
- 동시성 입찰 테스트(150건의 동시성 처리, 3개의 물품에 대해 각각 50건의 동시 입찰 요청)

## 테스트 페이지

1. IDE의 Live 기능 혹은 브라우저에서 파일열기로 `\index.html`을 실행해주세요.
2. 상품 목록을 확인할 수 있습니다.
3. 상품을 클릭하여 페이지 하단에서 상세 내용을 확인할 수 있습니다.
4. 상세 내용에서 입찰 혹은 즉시 구매를 할 수 있습니다.
5. 상세 내용에서 실시간으로 입찰 내역, 최고 입찰가, 경매 상태를 확인할 수 있습니다.
6. 테스트 페이지 상의 상품 상태는 1초마다 비동기적으로 업데이트 합니다.

## 아키텍처 및 기술 스택 상세 설명

### 이벤트 소싱 (Event Sourcing)

- 모든 상태 변경을 이벤트로 저장하여 시스템의 전체 히스토리를 유지합니다.
- 이벤트는 PostgreSQL 데이터베이스에 저장되며, Kafka를 통해 발행합니다.
- 이벤트 타입: `BidPlaced`, `BuyNowExecuted`

### CQRS (Command Query Responsibility Segregation)

- 명령(Command)과 조회(Query)를 분리하여 시스템의 확장성과 성능을 개선합니다.
- 명령: 입찰, 즉시 구매 등의 상태 변경 작업
- 조회: 경매 상태, 입찰 내역, 최고 입찰가 등의 정보 조회

### 동시성 제어

- 낙관적 동시성 제어(Optimistic Concurrency Control)를 사용하여 데이터 일관성을 유지합니다.
- 버전 관리를 통해 동시 업데이트 충돌을 감지하고 처리합니다.

### 스케일아웃

- Kafka를 사용하여 이벤트 기반 아키텍처를 구현, 시스템의 수평적 확장을 가능하게 합니다.
- 상품 ID를 기준으로 Kafka 파티셔닝을 수행하여 병렬 처리 능력을 향상합니다.

### 데이터베이스 설계

- PostgreSQL을 사용하여 관계형 데이터베이스 구조를 구현했습니다.
- 주요 테이블: items (상품 정보), bids (입찰 내역), events (이벤트 저장소)

### 오류 처리 및 재시도 메커니즘

- 낙관적 동시성 제어로 인한 충돌 발생 시 자동으로 재시도 합니다.
- 최대 재시도 횟수를 설정하여 무한 루프를 방지합니다.

### 테스트

- 통합 테스트를 통해 시스템의 주요 기능과 시나리오를 검증합니다.
- 동시성 테스트를 포함하여 API 호출을 통해 실제 운영 환경과 유사한 상황에서의 시스템 동작을 확인합니다.

### 코드 플로우

입찰에 대한 엔드포인트 호출부터 이벤트 실행까지의 흐름을 설명합니다.

  1. 엔드포인트 호출: 클라이언트가 `/bid` 엔드포인트로 POST 요청을 보냅니다.

      ``` javascript
      const response = await fetch(`${API_URL}/bid`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          item_id: ITEM_ID,
          bidder_id: parseInt(bidder_id),
          bid_amount: bid_amount,
        }),
      });
      ```

  2. 라우터 처리: `main.rs`에서 정의된 라우터가 요청을 `handlers::handle_bid` 함수로 전달합니다.

      ``` rust
      .route("/bid", post(handlers::handle_bid))
      ```

  3. 핸들러 처리: `handlers::handle_bid` 함수에서는 먼저 상품의 현재 경매 시간과 상태를 확인합니다.

      ``` rust
      let item = handlers::get_item(db_manager, cmd.item_id)
          .await
          .map_err(|e| serde_json::json!({"error": e.to_string()}))?;
      ```

  4. 명령 처리: `handle_bid` 함수 내에서 `handle_place_bid` 함수가 호출합니다. 이 함수는 `bidding/commands.rs`에 정의되어 있습니다.

      ``` rust
      /// 1. 입찰
      pub async fn handle_place_bid(
          cmd: PlaceBidCommand,
          event_store: &impl EventStore,
          db_manager: &DatabaseManager,
      ) -> Result<(), serde_json::Value> {
      ```

  5. 이벤트 저장소에 이벤트 저장: `handle_place_bid` 함수 내에서 입찰 이벤트가 생성되고 `event_store.append_and_publish_event` 메서드를 통해 저장합니다.

      ``` rust
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
      ```

  6. 이벤트 발행: `event_store/mod.rs`의 `append_and_publish_event` 메서드에서 이벤트가 데이터베이스에 저장되고 Kafka로 발행합니다.

      ``` rust
      async fn append_and_publish_event(&self, event: Event) -> Result<(), String> {
        let event_id = sqlx::query_scalar::<_, i64>(
      ```

  7. 이벤트 소비: `EventConsumer`가 Kafka에서 이벤트를 소비하고 처리합니다. 이는 `event_store/mod.rs`의 `start` 메서드에서 시작합니다.

      ``` rust
      /// 이벤트 소싱 시작
      pub async fn start(&self) {
        let db_manager = Arc::clone(&self.db_manager);
          if let Err(e) = self
              .kafka_consumer
              .consume_events("events", move |event| {
                  let db_manager = Arc::clone(&db_manager);
      ```

  8. 이벤트 처리: 소비된 이벤트는 `process_event` 메서드에서 처리합니다. 이벤트 타입에 따라 적절한 핸들러가 호출합니다.

      ``` rust
      /// 이벤트 처리
      async fn process_event(
        db_manager: &DatabaseManager,
          event: Event,
      ) -> Result<(), Box<dyn std::error::Error>> {
          match event.event_type.as_str() {
              "BidPlaced" => Self::handle_bid_placed(db_manager, &event).await?,
                "BuyNowExecuted" => Self::handle_buy_now_executed(db_manager, &event).await?,
                _ => warn!(
                    "{:<12} --> 알 수 없는 이벤트 타입: {}",
                    "EventConsume", event.event_type
                ),
            }
            Ok(())
      }
      ```

  9. 상태 업데이트: 이벤트 처리 결과로 데이터베이스의 상태가 업데이트합니다. 예를 들어, 입찰 이벤트의 경우 `handle_bid_placed` 메서드에서 처리합니다.

      ``` rust
      async fn handle_bid_placed(
        db_manager: &DatabaseManager,
        event: &Event,
      ) -> Result<(), Box<dyn std::error::Error>> {
        info!("{:<12} --> 입찰(BidPlaced)", "EventConsume");
        let bid_event: AuctionEvent = serde_json::from_value(event.data.clone())?;
        if let AuctionEvent::BidPlaced {
            item_id,
            bidder_id,
      ```
