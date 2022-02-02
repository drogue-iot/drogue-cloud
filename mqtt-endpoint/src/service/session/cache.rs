use clru::CLruCache;
use drogue_cloud_mqtt_common::error::PublishError;
use futures::lock::Mutex;
use std::{
    cell::Cell,
    fmt::Display,
    future::Future,
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, Instant},
};
use tracing::instrument;

struct DeviceCacheEntry<T> {
    pub device: Option<Arc<T>>,
    pub expired: Instant,
}

impl<T> DeviceCacheEntry<T> {
    fn new(device: Option<T>, ttl: Duration) -> Self {
        Self {
            device: device.map(Arc::new),
            expired: Instant::now() + ttl,
        }
    }

    pub fn expired(&self) -> bool {
        Instant::now() >= self.expired
    }
}

#[derive(Clone)]
pub struct DeviceCache<T> {
    cache: Arc<Mutex<CLruCache<String, DeviceCacheEntry<T>>>>,
    ttl: Duration,
    next_evict: Cell<Instant>,
}

impl<T> DeviceCache<T> {
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        let capacity = NonZeroUsize::new(capacity)
            .unwrap_or_else(|| unsafe { NonZeroUsize::new_unchecked(1) });
        let cache = Arc::new(Mutex::new(CLruCache::new(capacity)));
        Self {
            cache,
            ttl,
            next_evict: Cell::new(Instant::now()),
        }
    }

    #[instrument(skip(self, retriever),fields(self.ttl = ?self.ttl,self.next_evict=?self.next_evict))]
    pub async fn fetch<'f, F, Fut, E>(
        &self,
        as_device: &'f str,
        retriever: F,
    ) -> Result<Arc<T>, PublishError>
    where
        F: FnOnce(&'f str) -> Fut,
        Fut: Future<Output = Result<Option<T>, E>>,
        E: Display,
    {
        let mut cache = self.cache.lock().await;
        // let as_device = as_device.as_ref();
        match cache.get_mut(as_device) {
            // entry found, and not expired
            Some(outcome) if !outcome.expired() => match &outcome.device {
                Some(r#as) => Ok(r#as.clone()),
                _ => Err(PublishError::NotAuthorized),
            },
            // entry found, but expired
            Some(_) => {
                // remove the existing entry
                cache.pop(as_device);
                self.load_and_cache(as_device, &mut cache, retriever).await
            }
            // No cache entry found
            None => self.load_and_cache(as_device, &mut cache, retriever).await,
        }
    }

    /// Load device information and cache the outcome.
    #[instrument(skip(self,cache,retriever),fields(self.ttl = ?self.ttl,self.next_evict=?self.next_evict),err)]
    async fn load_and_cache<'f, F, Fut, E>(
        &self,
        as_device: &'f str,
        cache: &mut CLruCache<String, DeviceCacheEntry<T>>,
        retriever: F,
    ) -> Result<Arc<T>, PublishError>
    where
        F: FnOnce(&'f str) -> Fut,
        Fut: Future<Output = Result<Option<T>, E>>,
        E: Display,
    {
        let outcome = retriever(as_device).await.map_err(|err| {
            log::info!("Authorize as failed: {}", err);
            PublishError::InternalError("Failed to authorize device".into())
        })?;

        let entry = DeviceCacheEntry::new(outcome, self.ttl);
        let device = entry.device.clone();

        if cache.is_full() && self.next_evict.get() <= Instant::now() {
            self.next_evict.set(Instant::now() + self.ttl);
            Self::do_evict(cache).await;
        }

        cache.put(as_device.to_string(), entry);

        match device {
            Some(r#as) => Ok(r#as),
            None => Err(PublishError::NotAuthorized),
        }
    }

    #[allow(unused)]
    #[instrument(skip(self),fields(self.ttl = ?self.ttl,self.next_evict=?self.next_evict))]
    pub async fn evict(&self) {
        let mut cache = self.cache.lock().await;
        Self::do_evict(&mut cache).await;
    }

    async fn do_evict(cache: &mut CLruCache<String, DeviceCacheEntry<T>>) {
        cache.retain(|_, v| !v.expired());
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::ops::Deref;

    async fn fetch<S>(value: S) -> Result<Option<String>, String>
    where
        S: Into<String>,
    {
        Ok(Some(value.into()))
    }

    #[tokio::test]
    async fn test_capacity() {
        let cache = DeviceCache::<String>::new(5, Duration::from_secs(5));

        for i in 0..7 {
            let entry = cache
                .fetch(&format!("device{}", i), |_| fetch(format!("D{}", i)))
                .await;

            let expected = format!("D{}", i);
            assert_eq!(entry, Ok(Arc::new(expected)));
        }

        let cache_access = cache.cache.lock().await;
        assert_eq!(cache_access.len(), 5);
        assert!(cache_access.is_full());
        drop(cache_access);

        assert_values(
            &[Some("D6"), Some("D5"), Some("D4"), Some("D3"), Some("D2")],
            &cache,
        )
        .await;
    }

    #[tokio::test]
    async fn test_expiration() {
        let cache = DeviceCache::<String>::new(5, Duration::from_secs(2));

        // prime the cache with D1
        let entry = cache.fetch("device", |_| fetch("D1")).await;
        assert_eq!(entry, Ok(Arc::new("D1".to_string())));

        // not timed out yet, must still return D1
        let entry = cache.fetch("device", |_| fetch("D2")).await;
        assert_eq!(entry, Ok(Arc::new("D1".to_string())));

        // let the entry expire
        tokio::time::sleep(Duration::from_secs(2)).await;

        // timed out yet, must still return D3
        let entry = cache.fetch("device", |_| fetch("D3")).await;
        assert_eq!(entry, Ok(Arc::new("D3".to_string())));
    }

    #[tokio::test]
    async fn test_evict() {
        let cache = DeviceCache::<String>::new(5, Duration::from_secs(2));

        // prime the cache with D1 and D2
        let entry = cache.fetch("device1", |_| fetch("D1")).await;
        assert_eq!(entry, Ok(Arc::new("D1".to_string())));
        let entry = cache.fetch("device2", |_| fetch("D2")).await;
        assert_eq!(entry, Ok(Arc::new("D2".to_string())));

        assert_values(&[Some("D2"), Some("D1")], &cache).await;

        // let the entry expire
        tokio::time::sleep(Duration::from_secs(2)).await;

        // evict
        cache.evict().await;

        // assert again

        assert_values(&[], &cache).await;
    }

    #[tokio::test]
    async fn test_evict_when_full() {
        let cache = DeviceCache::<String>::new(5, Duration::from_secs(2));

        // full up the cache

        for i in 0..5 {
            let entry = cache
                .fetch(&format!("device{}", i), |_| fetch(format!("D{}", i)))
                .await;

            let expected = format!("D{}", i);
            assert_eq!(entry, Ok(Arc::new(expected)));
        }

        // ensure we have our 5 entries

        let cache_access = cache.cache.lock().await;
        assert_eq!(cache_access.len(), 5);
        assert!(cache_access.is_full());
        drop(cache_access);

        assert_values(
            &[Some("D4"), Some("D3"), Some("D2"), Some("D1"), Some("D0")],
            &cache,
        )
        .await;

        // let the entries expire

        tokio::time::sleep(Duration::from_secs(2)).await;

        // fetch one more, this must evict the cache

        let entry = cache.fetch(&"deviceX".to_string(), |_| fetch("DX")).await;
        assert_eq!(entry, Ok(Arc::new("DX".to_string())));

        // ensure we only have 1 entry now

        let cache_access = cache.cache.lock().await;
        assert_eq!(cache_access.len(), 1);
        assert!(!cache_access.is_full());
        drop(cache_access);

        assert_values(&[Some("DX")], &cache).await;
    }

    async fn assert_values(expected: &[Option<&str>], actual: &DeviceCache<String>) {
        assert_eq!(
            expected,
            actual
                .cache
                .lock()
                .await
                .iter()
                .map(|(_, v)| v.device.as_ref().map(|s| s.deref().deref()))
                .collect::<Vec<_>>()
        );
    }
}
