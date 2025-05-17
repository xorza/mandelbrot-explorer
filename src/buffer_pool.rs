use parking_lot::Mutex;
use std::sync::{atomic::{AtomicUsize, Ordering}, Arc, Weak};

#[derive(Debug)]
pub struct BufferPool {
    inner: Arc<BufferPoolInner>,
}

#[derive(Debug)]
struct BufferPoolInner {
    buf_size: usize,
    available: Mutex<Vec<Vec<u8>>>,
    total_allocated: AtomicUsize,
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
            let mut buf = self.data.lock();
            let mut avail = pool.available.lock();
            avail.push(std::mem::take(&mut *buf));
        }
    }
}

impl BufferPool {
    pub fn new(buf_size: usize, reserved_count: usize) -> Self {
        let inner = Arc::new(BufferPoolInner {
            buf_size,
            available: Mutex::new(Vec::new()),
            total_allocated: AtomicUsize::new(0),
        });

        {
            let mut avail = inner.available.lock();
            for _ in 0..reserved_count {
                avail.push(vec![0u8; buf_size]);
                inner.total_allocated.fetch_add(1, Ordering::Relaxed);
            }
        }

        Self { inner }
    }

    pub fn take(&self) -> Arc<BufferHandle> {
        let vec = self
            .inner
            .available
            .lock()
            .pop()
            .unwrap_or_else(|| {
                let new_total = self.inner.total_allocated.fetch_add(1, Ordering::Relaxed) + 1;
                if cfg!(debug_assertions) {
                    println!("Total allocated buffers: {}", new_total);
                }
                vec![0u8; self.inner.buf_size]
            });

        Arc::new(BufferHandle {
            data: Mutex::new(vec),
            pool: Arc::downgrade(&self.inner),
        })
    }

    pub(crate) fn taken_buffer_count(&self) -> u32 {
        let allocated = self.inner.total_allocated.load(Ordering::Relaxed);
        let available = self.inner.available.lock().len();
        (allocated - available) as u32
    }
}
