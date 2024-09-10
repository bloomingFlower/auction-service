use sqlx::postgres::{PgPool, PgPoolOptions};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub struct DatabaseManager {
    pub pool: Arc<PgPool>,
}

impl DatabaseManager {
    /// 데이터베이스 매니저 생성
    pub async fn new() -> Self {
        let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to create pool");
        Self {
            pool: Arc::new(pool),
        }
    }

    /// 데이터베이스 풀 가져오기
    pub fn get_pool(&self) -> Arc<PgPool> {
        Arc::clone(&self.pool)
    }

    /// 트랜잭션 실행
    pub async fn transaction<F, R, E>(&self, f: F) -> Result<R, E>
    where
        F: for<'c> FnOnce(
            &'c mut sqlx::Transaction<'_, sqlx::Postgres>,
        ) -> Pin<Box<dyn Future<Output = Result<R, E>> + Send + 'c>>,
        E: From<sqlx::Error>,
    {
        let mut tx = self.pool.begin().await?;
        let result = f(&mut tx).await;
        match result {
            Ok(r) => {
                tx.commit().await?;
                Ok(r)
            }
            Err(e) => {
                tx.rollback().await?;
                Err(e)
            }
        }
    }

    /// 데이터베이스 초기화
    pub async fn initialize_database(&self) -> Result<(), sqlx::Error> {
        // 00-recreate-db.sql 실행
        let recreate_db_sql = include_str!("../sql/00-recreate-db.sql");
        self.execute_multi_query(recreate_db_sql).await?;

        // 01-create-schema.sql 실행
        let create_schema_sql = include_str!("../sql/01-create-schema.sql");
        self.execute_multi_query(create_schema_sql).await?;

        Ok(())
    }

    /// 여러 쿼리 실행
    async fn execute_multi_query(&self, sql: &str) -> Result<(), sqlx::Error> {
        for query in sql.split(';') {
            let query = query.trim();
            if !query.is_empty() {
                sqlx::query(query).execute(&*self.pool).await?;
            }
        }
        Ok(())
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
