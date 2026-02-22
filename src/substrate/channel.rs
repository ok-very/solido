use ringbuf::{
    traits::{Consumer, Observer, Producer, Split},
    HeapCons, HeapProd, HeapRb,
};

/// Lock-free single-producer single-consumer channel built on ringbuf.
///
/// Used for inter-thread comms: audio↔control, camera↔control, LLM↔control.
/// No allocations after creation. No mutexes.
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let rb = HeapRb::<T>::new(capacity);
    let (prod, cons) = rb.split();
    (Sender { inner: prod }, Receiver { inner: cons })
}

pub struct Sender<T> {
    inner: HeapProd<T>,
}

pub struct Receiver<T> {
    inner: HeapCons<T>,
}

impl<T> Sender<T> {
    /// Try to push a value. Returns Err(value) if the buffer is full.
    /// Non-blocking — safe for audio callbacks.
    pub fn try_send(&mut self, value: T) -> Result<(), T> {
        self.inner.try_push(value)
    }

    /// Number of slots available for writing.
    pub fn available(&self) -> usize {
        self.inner.vacant_len()
    }
}

impl<T> Receiver<T> {
    /// Try to pop a value. Returns None if empty.
    /// Non-blocking — safe for audio callbacks.
    pub fn try_recv(&mut self) -> Option<T> {
        self.inner.try_pop()
    }

    /// Drain all available messages into a vec.
    /// Useful for the control thread to batch-process commands.
    pub fn drain(&mut self) -> Vec<T> {
        let mut out = Vec::new();
        while let Some(v) = self.inner.try_pop() {
            out.push(v);
        }
        out
    }

    /// Number of items available for reading.
    pub fn available(&self) -> usize {
        self.inner.occupied_len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_recv_round_trip() {
        let (mut tx, mut rx) = channel::<i32>(4);
        tx.try_send(42).unwrap();
        tx.try_send(99).unwrap();
        assert_eq!(rx.try_recv(), Some(42));
        assert_eq!(rx.try_recv(), Some(99));
        assert_eq!(rx.try_recv(), None);
    }

    #[test]
    fn full_buffer_returns_err() {
        let (mut tx, mut _rx) = channel::<i32>(2);
        assert!(tx.try_send(1).is_ok());
        assert!(tx.try_send(2).is_ok());
        assert!(tx.try_send(3).is_err());
    }

    #[test]
    fn drain_collects_all() {
        let (mut tx, mut rx) = channel::<i32>(8);
        for i in 0..5 {
            tx.try_send(i).unwrap();
        }
        let drained = rx.drain();
        assert_eq!(drained, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn cross_thread() {
        let (mut tx, mut rx) = channel::<String>(4);
        let handle = std::thread::spawn(move || {
            tx.try_send("hello".into()).unwrap();
        });
        handle.join().unwrap();
        assert_eq!(rx.try_recv(), Some("hello".into()));
    }
}
