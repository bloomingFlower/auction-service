use sqlx::postgres::{PgPool, PgPoolOptions};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

use crate::error::{AppError, Result};

pub struct DatabaseManager {
    pub pool: Arc<PgPool>,
}

impl DatabaseManager {
    /// 데이터베이스 매니저 생성
    pub async fn new() -> Self {
        let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

        // 연결 풀 설정 최적화
        let pool = PgPoolOptions::new()
            .max_connections(20) // 최대 연결 수 증가
            .min_connections(5) // 최소 연결 수 설정
            .max_lifetime(Duration::from_secs(1800)) // 연결 최대 수명 30분
            .idle_timeout(Duration::from_secs(600)) // 유휴 타임아웃 10분
            .acquire_timeout(Duration::from_secs(30)) // 획득 타임아웃 30초
            .connect(&database_url)
            .await
            .expect("Failed to create pool");

        info!("Database connection pool created with max_connections=20, min_connections=5");

        Self {
            pool: Arc::new(pool),
        }
    }

    /// 데이터베이스 풀 가져오기
    pub fn get_pool(&self) -> Arc<PgPool> {
        Arc::clone(&self.pool)
    }

    /// 트랜잭션 실행
    pub async fn transaction<F, R>(&self, f: F) -> Result<R>
    where
        F: for<'c> FnOnce(
            &'c mut sqlx::Transaction<'_, sqlx::Postgres>,
        ) -> Pin<Box<dyn Future<Output = Result<R>> + Send + 'c>>,
    {
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        let result = f(&mut tx).await;
        match result {
            Ok(r) => {
                tx.commit().await.map_err(AppError::from)?;
                Ok(r)
            }
            Err(e) => {
                if let Err(rollback_err) = tx.rollback().await {
                    info!("트랜잭션 롤백 실패: {:?}", rollback_err);
                }
                Err(e)
            }
        }
    }

    /// SQL 쿼리 실행 (단일 쿼리)
    pub async fn execute(&self, query: &str) -> Result<()> {
        sqlx::query(query)
            .execute(&*self.pool)
            .await
            .map(|_| ())
            .map_err(AppError::from)
    }

    /// 데이터베이스 초기화
    pub async fn initialize_database(&self) -> Result<()> {
        // 00-recreate-db.sql 실행
        let recreate_db_sql = include_str!("../sql/00-recreate-db.sql");
        self.execute_multi_query(recreate_db_sql).await?;

        // 01-create-schema.sql 실행
        let create_schema_sql = include_str!("../sql/01-create-schema.sql");
        self.execute_multi_query(create_schema_sql).await?;

        Ok(())
    }

    /// 여러 쿼리 실행
    async fn execute_multi_query(&self, sql: &str) -> Result<()> {
        for query in sql.split(';') {
            let query = query.trim();
            if !query.is_empty() {
                self.execute(query).await?;
            }
        }
        Ok(())
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
