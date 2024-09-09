// region:    --- Imports
use crate::event_store::Event;
use rdkafka::admin::{AdminClient, AdminOptions, NewTopic, TopicReplication};
use rdkafka::client::DefaultClientContext;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::Message;
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::ClientConfig;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::{debug, error, info, warn};

// endregion: --- Imports

// region:    --- Kafka Producer
#[derive(Clone)]
pub struct KafkaProducer {
    producer: Arc<FutureProducer>,
}

/// KafkaProducer 구현
impl KafkaProducer {
    pub fn new(brokers: &str) -> Self {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .create()
            .expect("Producer creation error");

        KafkaProducer {
            producer: Arc::new(producer),
        }
    }

    /// 메시지 전송
    pub async fn send_message(&self, topic: &str, key: &str, value: &str) -> Result<(), String> {
        info!(
            "{:<12} --> Kafka 메시지 전송: topic={}, key={}",
            "Producer", topic, key
        );
        let record = FutureRecord::to(topic).key(key).payload(value);

        self.producer
            .send(record, std::time::Duration::from_secs(0))
            .await
            .map_err(|(e, _)| format!("Error sending message: {:?}", e))?;

        Ok(())
    }
}

// endregion: --- Kafka Producer

// region:    --- Kafka Consumer
pub struct KafkaConsumer {
    consumer: Arc<StreamConsumer>,
}

/// KafkaConsumer 구현
impl KafkaConsumer {
    pub fn new(brokers: &str, group_id: &str) -> Self {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", group_id)
            .set("enable.auto.commit", "true")
            .set("auto.offset.reset", "earliest")
            .set("session.timeout.ms", "6000")
            .set("fetch.max.bytes", "5242880")
            .set("allow.auto.create.topics", "true")
            .create()
            .expect("Consumer creation failed");

        KafkaConsumer {
            consumer: Arc::new(consumer),
        }
    }

    /// 이벤트 소싱
    pub async fn consume_events<F, Fut>(
        &self,
        topic: &str,
        handler: F,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: Fn(Event) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + 'static,
    {
        info!(
            "{:<12} --> Kafka 이벤트 소싱 시작: topic={}",
            "Consumer", topic
        );
        self.consumer.subscribe(&[topic])?;

        loop {
            match self.consumer.recv().await {
                Ok(message) => {
                    info!(
                        "{:<12} --> 메시지 수신: topic={}, partition={}, offset={}",
                        "Consumer",
                        message.topic(),
                        message.partition(),
                        message.offset()
                    );

                    if let Some(payload) = message.payload() {
                        match serde_json::from_slice::<Event>(payload) {
                            Ok(event) => {
                                debug!("{:<12} --> deserialize 성공: {:?}", "Consumer", event);

                                if let Err(e) = handler(event).await {
                                    error!(
                                        "{:<12} --> Kafka 이벤트 처리 오류: {:?}",
                                        "Consumer", e
                                    );
                                } else {
                                    info!("{:<12} --> Kafka 이벤트 처리 성공", "Consumer");
                                }
                            }
                            Err(e) => error!("{:<12} --> deserialize 오류: {:?}", "Consumer", e),
                        }
                    } else {
                        warn!("{:<12} --> 빈 페이로드 수신", "Consumer");
                    }
                }
                Err(e) => error!("{:<12} --> 메시지 수신 오류: {:?}", "Consumer", e),
            }
        }
    }
}

// endregion: --- Kafka Consumer

// region:    --- Kafka Manager
pub struct KafkaManager {
    producer: Arc<KafkaProducer>,
    consumer: Arc<KafkaConsumer>,
    brokers: String,
}

impl Default for KafkaManager {
    fn default() -> Self {
        Self::new()
    }
}

/// KafkaManager 구현
impl KafkaManager {
    pub fn new() -> Self {
        let brokers =
            std::env::var("KAFKA_BROKERS").unwrap_or_else(|_| "localhost:9092".to_string());
        let group_id = "events-group".to_string();

        let producer = Arc::new(KafkaProducer::new(&brokers));
        let consumer = Arc::new(KafkaConsumer::new(&brokers, &group_id));

        KafkaManager {
            producer,
            consumer,
            brokers,
        }
    }

    /// 프로듀서 반환
    pub fn get_producer(&self) -> Arc<KafkaProducer> {
        Arc::clone(&self.producer)
    }

    /// 컨슈머 반환
    pub fn get_consumer(&self) -> Arc<KafkaConsumer> {
        Arc::clone(&self.consumer)
    }

    /// 초기화 메시지 전송
    pub async fn send_init_message(&self) -> Result<(), String> {
        info!("{:<12} --> Kafka 초기화 메시지 전송", "Manager");
        self.producer
            .send_message("init-topic", "init-key", "init-message")
            .await
    }

    /// Kafka 초기화
    pub async fn initialize(&self) -> Result<(), String> {
        info!("{:<12} --> Kafka 초기화 시작", "Manager");

        // 초기화 토픽 구
        self.consumer
            .consumer
            .subscribe(&["init-topic"])
            .map_err(|e| e.to_string())?;

        // 초기화 메시지 전송
        self.send_init_message().await?;

        // 초기화 메시지 수신 대기
        let mut attempts = 0;
        let max_attempts = 10;
        while attempts < max_attempts {
            match time::timeout(Duration::from_secs(1), self.consumer.consumer.recv()).await {
                Ok(Ok(message)) => {
                    if let Some(payload) = message.payload() {
                        if payload == b"init-message" {
                            info!("{:<12} --> Kafka 초기화 메시지 수신 확인", "Manager");
                            return Ok(());
                        }
                    }
                }
                Ok(Err(e)) => error!(
                    "{:<12} --> Kafka 초기화 메시지 수신 오류: {:?}",
                    "Manager", e
                ),
                Err(_) => {
                    attempts += 1;
                    warn!(
                        "{:<12} --> Kafka 초기화 메시지 수신 대기 중... (시도: {}/{})",
                        "Manager", attempts, max_attempts
                    );
                }
            }
        }

        Err("Kafka 초기화 메시지 수신 실패".to_string())
    }

    /// 토픽 생성
    pub async fn create_topic(
        &self,
        topic_name: &str,
        num_partitions: i32,
        replication_factor: i32,
    ) -> Result<(), String> {
        info!("{:<12} --> Kafka 토픽 생성 시작: {}", "Manager", topic_name);

        let admin_client: AdminClient<DefaultClientContext> = ClientConfig::new()
            .set("bootstrap.servers", &self.brokers)
            .create()
            .map_err(|e| format!("AdminClient 생성 실패: {:?}", e))?;

        let new_topic = NewTopic::new(
            topic_name,
            num_partitions,
            TopicReplication::Fixed(replication_factor),
        );

        match admin_client
            .create_topics(&[new_topic], &AdminOptions::new())
            .await
        {
            Ok(_) => {
                info!("{:<12} --> Kafka 토픽 생성 성공: {}", "Manager", topic_name);
                Ok(())
            }
            Err(e) => {
                error!("{:<12} --> Kafka 토픽 생성 실패: {:?}", "Manager", e);
                Err(format!("토픽 생성 실패: {:?}", e))
            }
        }
    }
}

// endregion: --- Kafka Manager
