use parking_lot::Mutex;
use std::sync::{Arc, Weak};

/// Recycling free-list of fixed-size byte buffers. Buffers return themselves to
/// the pool when their `BufferHandle` is dropped; the pool grows on demand and
/// never shrinks, so steady-state allocation tracks the working set.
#[derive(Debug)]
pub struct BufferPool {
    inner: Arc<BufferPoolInner>,
}

#[derive(Debug)]
struct BufferPoolInner {
    buf_size: usize,
    available: Mutex<Vec<Vec<u8>>>,
}

#[derive(Debug)]
pub struct BufferHandle {
    data: Mutex<Vec<u8>>,
    pool: Weak<BufferPoolInner>,
}

impl BufferHandle {
    pub fn lock(&self) -> parking_lot::MutexGuard<'_, Vec<u8>> {
        self.data.lock()
    }
}

impl Drop for BufferHandle {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.upgrade() {
            pool.available
                .lock()
                .push(std::mem::take(&mut *self.data.lock()));
        }
    }
}

impl BufferPool {
    pub fn new(buf_size: usize, reserved_count: usize) -> Self {
        let available = (0..reserved_count).map(|_| vec![0u8; buf_size]).collect();
        Self {
            inner: Arc::new(BufferPoolInner {
                buf_size,
                available: Mutex::new(available),
            }),
        }
    }

    pub fn take(&self) -> Arc<BufferHandle> {
        let vec = self
            .inner
            .available
            .lock()
            .pop()
            .unwrap_or_else(|| vec![0u8; self.inner.buf_size]);
        Arc::new(BufferHandle {
            data: Mutex::new(vec),
            pool: Arc::downgrade(&self.inner),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recycles_buffers_and_grows_past_reserve() {
        let pool = BufferPool::new(16, 1);

        // The single reserved buffer is handed out first.
        let a = pool.take();
        assert_eq!(a.lock().len(), 16);

        // Pool is now empty; taking again grows it instead of panicking.
        let b = pool.take();
        assert_eq!(b.lock().len(), 16);

        // Mark `a`, drop it back into the pool.
        a.lock()[0] = 7;
        drop(a);

        // The next take must reuse `a`'s exact buffer (data preserved proves
        // recycling, not a fresh zeroed allocation).
        let c = pool.take();
        assert_eq!(c.lock()[0], 7);

        drop(b);
        drop(c);
    }
}
