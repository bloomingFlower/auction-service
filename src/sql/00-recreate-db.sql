-- 데이터베이스 연결 종료
SELECT pg_terminate_backend(pg_stat_activity.pid)
FROM pg_stat_activity
WHERE pg_stat_activity.datname = 'auction';
-- 데이터베이스 삭제
DROP DATABASE IF EXISTS auction;
-- 사용자 삭제
DROP USER IF EXISTS demo;
-- 사용자 생성
CREATE USER demo PASSWORD 'demo';
-- 데이터베이스 생성
CREATE DATABASE auction owner demo ENCODING = 'UTF8';