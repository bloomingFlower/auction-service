-- Create the schema

-- 시퀀스 생성
CREATE SEQUENCE IF NOT EXISTS items_id_seq;
CREATE SEQUENCE IF NOT EXISTS bids_id_seq;
CREATE SEQUENCE IF NOT EXISTS events_id_seq;

-- Items 테이블 생성
CREATE TABLE IF NOT EXISTS items (
   id BIGINT PRIMARY KEY DEFAULT nextval('items_id_seq'),
   title TEXT NOT NULL,
   description TEXT,
   starting_price BIGINT NOT NULL,
   current_price BIGINT NOT NULL,
   buy_now_price BIGINT NOT NULL,
   start_time TIMESTAMP WITH TIME ZONE NOT NULL,
   end_time TIMESTAMP WITH TIME ZONE NOT NULL,
   seller TEXT NOT NULL,
   status TEXT NOT NULL CHECK (status IN ('SCHEDULED', 'ACTIVE', 'COMPLETED')),
   created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Bids 테이블 생성
CREATE TABLE IF NOT EXISTS bids (
   id BIGINT PRIMARY KEY DEFAULT nextval('bids_id_seq'),
   item_id BIGINT NOT NULL,
   bid_time TIMESTAMP WITH TIME ZONE NOT NULL,
   bidder_id BIGINT NOT NULL,
   bid_amount BIGINT NOT NULL,
   FOREIGN KEY (item_id) REFERENCES items(id)
);

-- Events 테이블 생성
CREATE TABLE IF NOT EXISTS events (
   id BIGINT PRIMARY KEY DEFAULT nextval('events_id_seq'),
   aggregate_id BIGINT NOT NULL,
   event_type TEXT NOT NULL,
   data JSONB NOT NULL,
   version BIGINT NOT NULL,
   timestamp TIMESTAMP WITH TIME ZONE NOT NULL,
   UNIQUE (aggregate_id, version)
);

-- 인덱스 생성
-- 기본 인덱스
CREATE INDEX IF NOT EXISTS idx_bids_item_id ON bids(item_id);
CREATE INDEX IF NOT EXISTS idx_bids_bid_time ON bids(bid_time DESC);
CREATE INDEX IF NOT EXISTS idx_events_aggregate_id_version ON events(aggregate_id, version);
CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);

-- 성능 최적화를 위한 추가 인덱스
CREATE INDEX IF NOT EXISTS idx_items_status ON items(status);
CREATE INDEX IF NOT EXISTS idx_items_status_end_time ON items(status, end_time);
CREATE INDEX IF NOT EXISTS idx_items_status_start_time ON items(status, start_time);
CREATE INDEX IF NOT EXISTS idx_bids_bidder_id ON bids(bidder_id);
CREATE INDEX IF NOT EXISTS idx_bids_item_id_bid_amount ON bids(item_id, bid_amount DESC);
CREATE INDEX IF NOT EXISTS idx_events_event_type ON events(event_type);

-- 전체 텍스트 검색을 위한 인덱스
CREATE INDEX IF NOT EXISTS idx_items_title_gin ON items USING gin(to_tsvector('english', title));
CREATE INDEX IF NOT EXISTS idx_items_description_gin ON items USING gin(to_tsvector('english', description));

-- 테스트 데이터 삽입
INSERT INTO items (title, description, starting_price, current_price, buy_now_price, start_time, end_time, seller, status, created_at)
VALUES (
  '나이키 한정판 운동화',
  'ACTIVE 테스트 - 칸예 웨스트 콜라보레이션',
   10000,
   10000,
   50000,
  NOW(),
  NOW() + INTERVAL '10 minutes',
  '일이삼',
  'ACTIVE',
  CURRENT_TIMESTAMP
);

INSERT INTO items (title, description, starting_price, current_price, buy_now_price, start_time, end_time, seller, status, created_at)
VALUES (
  '기안84 무제',
  'SCHEDULED 테스트 - 유명 웹툰 작가이자 방송인, 2023년작',
  50000,
  50000,
  100000,
  NOW() + INTERVAL '1 minutes',
  NOW() + INTERVAL '3 minutes',
  '버나스리',
  'SCHEDULED',
  CURRENT_TIMESTAMP
);

INSERT INTO items (title, description, starting_price, current_price, buy_now_price, start_time, end_time, seller, status, created_at)
VALUES (
  '빈티지 스피커',
  'COMPLETED 테스트 - 탄노이 빈티지 스피커',
  70000,
  70000,
  200000,
  NOW() - INTERVAL '10 minutes',
  NOW() - INTERVAL '3 minutes',
  '브라운김',
  'COMPLETED',
  CURRENT_TIMESTAMP
);