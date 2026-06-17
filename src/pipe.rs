use spin::Mutex;

pub const PIPE_BUF_SIZE: usize = 4096;
pub const MAX_PIPES: usize = 128;

#[derive(Copy, Clone, PartialEq)]
pub enum PipeEnd {
    Read,
    Write,
}

#[derive(Copy, Clone)]
pub struct Pipe {
    pub buffer: [u8; PIPE_BUF_SIZE],
    pub read_pos: usize,
    pub write_pos: usize,
    pub bytes: usize,
    pub read_open: bool,
    pub write_open: bool,
    pub id: u32,
}

impl Pipe {
    pub const fn new(id: u32) -> Self {
        Pipe {
            buffer: [0; PIPE_BUF_SIZE],
            read_pos: 0,
            write_pos: 0,
            bytes: 0,
            read_open: true,
            write_open: true,
            id,
        }
    }

    pub fn write(&mut self, data: &[u8]) -> usize {
        let mut written = 0;
        for &byte in data {
            if self.bytes >= PIPE_BUF_SIZE {
                break;
            }
            self.buffer[self.write_pos] = byte;
            self.write_pos = (self.write_pos + 1) % PIPE_BUF_SIZE;
            self.bytes += 1;
            written += 1;
        }
        written
    }

    pub fn read_byte(&mut self) -> Option<u8> {
        if self.bytes == 0 {
            return None;
        }
        let byte = self.buffer[self.read_pos];
        self.read_pos = (self.read_pos + 1) % PIPE_BUF_SIZE;
        self.bytes -= 1;
        Some(byte)
    }

    pub fn read(&mut self, data: &mut [u8]) -> usize {
        let mut read = 0;
        for byte in data.iter_mut() {
            match self.read_byte() {
                Some(b) => {
                    *byte = b;
                    read += 1;
                }
                None => break,
            }
        }
        read
    }

    pub fn available(&self) -> usize {
        self.bytes
    }

    pub fn close_read(&mut self) {
        self.read_open = false;
    }

    pub fn close_write(&mut self) {
        self.write_open = false;
    }

    pub fn closed(&self) -> bool {
        !self.read_open && !self.write_open
    }
}

pub struct PipeManager {
    pub pipes: [Option<Pipe>; MAX_PIPES],
    pub next_id: u32,
}

impl PipeManager {
    pub const fn new() -> Self {
        PipeManager { pipes: [None; MAX_PIPES], next_id: 1 }
    }

    pub fn create(&mut self) -> Option<(u32, u32)> {
        for i in 0..MAX_PIPES {
            if self.pipes[i].is_none() {
                let id = self.next_id;
                self.next_id += 1;
                self.pipes[i] = Some(Pipe::new(id));
                return Some((id, id));
            }
        }
        None
    }

    pub fn destroy(&mut self, id: u32) -> bool {
        for i in 0..MAX_PIPES {
            if let Some(pipe) = &self.pipes[i] {
                if pipe.id == id {
                    self.pipes[i] = None;
                    return true;
                }
            }
        }
        false
    }

    pub fn get_pipe(&mut self, id: u32) -> Option<&mut Pipe> {
        let i = self.pipes.iter().position(|p| {
            p.as_ref().map_or(false, |pipe| pipe.id == id)
        })?;
        self.pipes[i].as_mut()
    }

    pub fn write(&mut self, id: u32, data: &[u8]) -> Option<usize> {
        self.get_pipe(id).map(|pipe| pipe.write(data))
    }

    pub fn read(&mut self, id: u32, data: &mut [u8]) -> Option<usize> {
        self.get_pipe(id).map(|pipe| pipe.read(data))
    }

    pub fn available(&self, id: u32) -> usize {
        for i in 0..MAX_PIPES {
            if let Some(ref pipe) = self.pipes[i] {
                if pipe.id == id {
                    return pipe.available();
                }
            }
        }
        0
    }
}

pub static PIPE_MANAGER: Mutex<PipeManager> = Mutex::new(PipeManager::new());

pub fn init_pipes() {
    println!("Pipes: initialises");
}

pub mod fifo {
    

    #[derive(Copy, Clone)]
    pub struct FifoBuffer {
        pub data: [u8; 4096],
        pub head: usize,
        pub tail: usize,
        pub count: usize,
    }

    impl FifoBuffer {
        pub const fn new() -> Self {
            FifoBuffer { data: [0; 4096], head: 0, tail: 0, count: 0 }
        }

        pub fn push(&mut self, byte: u8) -> bool {
            if self.count >= 4096 {
                return false;
            }
            self.data[self.head] = byte;
            self.head = (self.head + 1) % 4096;
            self.count += 1;
            true
        }

        pub fn pop(&mut self) -> Option<u8> {
            if self.count == 0 {
                return None;
            }
            let byte = self.data[self.tail];
            self.tail = (self.tail + 1) % 4096;
            self.count -= 1;
            Some(byte)
        }

        pub fn available(&self) -> usize {
            self.count
        }

        pub fn is_full(&self) -> bool {
            self.count >= 4096
        }

        pub fn is_empty(&self) -> bool {
            self.count == 0
        }

        pub fn flush(&mut self) {
            self.head = 0;
            self.tail = 0;
            self.count = 0;
        }
    }
}

pub mod message_queue {
    
    use spin::Mutex;

    pub const MAX_QUEUES: usize = 32;
    pub const MAX_MSG_SIZE: usize = 256;
    pub const MAX_MSGS: usize = 64;

    #[derive(Copy, Clone)]
    pub struct Message {
        pub data: [u8; MAX_MSG_SIZE],
        pub len: usize,
        pub msg_type: u32,
    }

    #[derive(Copy, Clone)]
    pub struct MessageQueue {
        pub msgs: [Option<Message>; MAX_MSGS],
        pub count: usize,
        pub read_idx: usize,
        pub write_idx: usize,
        pub id: u32,
    }

    impl MessageQueue {
        pub const fn new(id: u32) -> Self {
            const EMPTY_MSG: Option<Message> = None;
            MessageQueue {
                msgs: [EMPTY_MSG; MAX_MSGS],
                count: 0,
                read_idx: 0,
                write_idx: 0,
                id,
            }
        }

        pub fn send(&mut self, data: &[u8], msg_type: u32) -> bool {
            if self.count >= MAX_MSGS || data.len() > MAX_MSG_SIZE {
                return false;
            }
            let mut msg = Message {
                data: [0; MAX_MSG_SIZE],
                len: data.len(),
                msg_type,
            };
            msg.data[..data.len()].copy_from_slice(data);
            self.msgs[self.write_idx] = Some(msg);
            self.write_idx = (self.write_idx + 1) % MAX_MSGS;
            self.count += 1;
            true
        }

        pub fn receive(&mut self) -> Option<Message> {
            if self.count == 0 {
                return None;
            }
            let msg = self.msgs[self.read_idx].take();
            self.read_idx = (self.read_idx + 1) % MAX_MSGS;
            self.count -= 1;
            msg
        }
    }

    pub struct MessageQueueManager {
        pub queues: [Option<MessageQueue>; MAX_QUEUES],
        pub next_id: u32,
    }

    impl MessageQueueManager {
        pub const fn new() -> Self {
            MessageQueueManager { queues: [None; MAX_QUEUES], next_id: 1 }
        }

        pub fn create(&mut self) -> Option<u32> {
            for i in 0..MAX_QUEUES {
                if self.queues[i].is_none() {
                    let id = self.next_id;
                    self.next_id += 1;
                    self.queues[i] = Some(MessageQueue::new(id));
                    return Some(id);
                }
            }
            None
        }

        pub fn destroy(&mut self, id: u32) -> bool {
            for i in 0..MAX_QUEUES {
                if let Some(ref q) = self.queues[i] {
                    if q.id == id {
                        self.queues[i] = None;
                        return true;
                    }
                }
            }
            false
        }

        pub fn send(&mut self, id: u32, data: &[u8], msg_type: u32) -> bool {
            self.get_queue(id).map(|q| q.send(data, msg_type)).unwrap_or(false)
        }

        pub fn receive(&mut self, id: u32) -> Option<Message> {
            self.get_queue(id).and_then(|q| q.receive())
        }

        fn get_queue(&mut self, id: u32) -> Option<&mut MessageQueue> {
            let i = self.queues.iter().position(|q| {
                q.as_ref().map_or(false, |mq| mq.id == id)
            })?;
            self.queues[i].as_mut()
        }
    }

    pub static MSG_QUEUE_MGR: Mutex<MessageQueueManager> = Mutex::new(MessageQueueManager::new());

    pub fn init_msg_queues() {
        println!("Message queues: initialisees");
    }
}