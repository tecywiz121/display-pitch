use std::{
    collections::VecDeque,
    sync::mpsc::{self, RecvError},
};

const QUEUE_CAPACITY: usize = 1024;
const BUFFER_MIN_CAPACITY: usize = 1 * 1024 * 1024;

pub struct Consumer<T> {
    receiver: mpsc::Receiver<Box<[T]>>,
    buffer: VecDeque<T>,
}

impl<T> Consumer<T> {
    fn new(receiver: mpsc::Receiver<Box<[T]>>) -> Self {
        Self {
            receiver,
            buffer: VecDeque::with_capacity(BUFFER_MIN_CAPACITY),
        }
    }
}

impl<T: Copy> Consumer<T> {
    pub fn read(&mut self, n: usize) -> Result<Vec<T>, RecvError> {
        while self.buffer.len() < n {
            self.buffer.extend(self.receiver.recv()?.iter().copied());
        }

        let rest = self.buffer.split_off(n);
        let head = std::mem::replace(&mut self.buffer, rest);
        Ok(head.into())
    }
}

pub struct Producer<T> {
    sender: mpsc::SyncSender<Box<[T]>>,
}

impl<T: Copy> Producer<T> {
    pub fn write(&self, data: &[T]) {
        self.sender.try_send(data.into()).ok();
    }
}

pub fn new<T>() -> (Producer<T>, Consumer<T>) {
    let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);

    let producer = Producer { sender };
    let consumer = Consumer::new(receiver);

    (producer, consumer)
}
