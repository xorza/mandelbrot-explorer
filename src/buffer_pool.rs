pub struct BufferPool {
    buf_size: usize,
}

impl BufferPool {
    pub fn new(buf_size: usize) -> Self {
        Self { buf_size }
    }

    pub fn take(&mut self) -> Vec<u8> {
        vec![0u8; self.buf_size]
    }
    pub fn release(&mut self, _buf: Vec<u8>) {
        // do nothing
    }
}
