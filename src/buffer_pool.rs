use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct BufferPool {
    buf_size: usize,
    buffers: Vec<Arc<Mutex<Vec<u8>>>>,
    total_allocated: usize,
}

impl BufferPool {
    pub fn new(buf_size: usize, reserved_count: usize) -> Self {
        let buffers = (0..reserved_count)
            .map(|_| Arc::new(Mutex::new(vec![0u8; buf_size])))
            .collect();

        Self {
            buf_size,
            buffers,
            total_allocated: reserved_count,
        }
    }

    pub fn take(&mut self) -> Arc<Mutex<Vec<u8>>> {
        if let Some(buf) = self.buffers.iter().find(|buf| Arc::strong_count(buf) == 1) {
            buf.clone()
        } else {
            self.total_allocated += 1;
            println!("Total allocated buffers: {}", self.total_allocated);

            self.buffers
                .push(Arc::new(Mutex::new(vec![0u8; self.buf_size])));
            self.buffers.last().unwrap().clone()
        }
    }

    pub(crate) fn taken_buffer_count(&self) -> u32 {
        self.buffers
            .iter()
            .filter(|buf| Arc::strong_count(buf) > 1)
            .count() as u32
    }
}
