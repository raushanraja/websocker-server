use r2d2_redis::redis::{Commands, FromRedisValue};
use r2d2_redis::{r2d2, RedisConnectionManager};
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum R2D2Error {
    #[error("could not get redis connection from pool : {0}")]
    RedisPoolError(r2d2_redis::r2d2::Error),
    #[error("error parsing string from redis result: {0}")]
    RedisTypeError(r2d2_redis::redis::RedisError),
    #[error("error executing redis command: {0}")]
    RedisCMDError(r2d2_redis::redis::RedisError),
    #[error("error creating Redis client: {0}")]
    RedisClientError(r2d2_redis::redis::RedisError),
}

pub type R2D2Pool = r2d2::Pool<RedisConnectionManager>;
pub type R2D2Con = r2d2::PooledConnection<RedisConnectionManager>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("r2d2 error: {0}")]
    R2D2Error(#[from] R2D2Error),
}

type Result<T> = std::result::Result<T, Error>;

const CACHE_POOL_MAX_OPEN: u32 = 16;
const CACHE_POOL_MIN_IDLE: u32 = 8;
const CACHE_POOL_TIMEOUT_SECONDS: u64 = 1;
const CACHE_POOL_EXPIRE_SECONDS: u64 = 60;
const REDIS_CON_STRING: &str = "redis://localhost:6379";

pub fn connect() -> Result<r2d2::Pool<RedisConnectionManager>> {
    let manager =
        RedisConnectionManager::new(REDIS_CON_STRING).map_err(R2D2Error::RedisClientError)?;
    r2d2::Pool::builder()
        .max_size(CACHE_POOL_MAX_OPEN)
        .max_lifetime(Some(Duration::from_secs(CACHE_POOL_EXPIRE_SECONDS)))
        .min_idle(Some(CACHE_POOL_MIN_IDLE))
        .build(manager)
        .map_err(|e| R2D2Error::RedisPoolError(e).into())
}

pub fn get_con(pool: &R2D2Pool) -> Result<R2D2Con> {
    pool.get_timeout(Duration::from_secs(CACHE_POOL_TIMEOUT_SECONDS))
        .map_err(|e| {
            eprintln!("error connecting to redis: {}", e);
            R2D2Error::RedisPoolError(e).into()
        })
}

pub fn set_str(pool: &R2D2Pool, key: &str, value: &str, ttl_seconds: usize) -> Result<()> {
    let mut con = get_con(&pool)?;
    con.set(key, value).map_err(R2D2Error::RedisCMDError)?;
    if ttl_seconds > 0 {
        con.expire(key, ttl_seconds)
            .map_err(R2D2Error::RedisCMDError)?;
    }
    Ok(())
}

pub fn get_str(pool: &R2D2Pool, key: &str) -> Result<String> {
    let mut con = get_con(&pool)?;
    let value = con.get(key).map_err(R2D2Error::RedisCMDError)?;
    FromRedisValue::from_redis_value(&value).map_err(|e| R2D2Error::RedisTypeError(e).into())
}
