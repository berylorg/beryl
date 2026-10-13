use super::*;
use std::io::Cursor;

pub(crate) struct DiagnosticStdioClient {
    input: Mutex<Option<SyncSender<Vec<u8>>>>,
    output: Mutex<Receiver<Vec<u8>>>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
    terminal: Arc<AtomicBool>,
}

impl DiagnosticStdioClient {
    pub(crate) fn terminal(&self) -> bool {
        self.terminal.load(Ordering::Acquire)
    }
    pub(crate) fn request(&self, value: &serde_json::Value) -> serde_json::Value {
        let mut frame = serde_json::to_vec(value).unwrap();
        assert!(frame.len() < MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES);
        frame.push(b'\n');
        self.input
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .send(frame)
            .unwrap();
        let response = self
            .output
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(response.len() <= MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES);
        serde_json::from_slice(&response).unwrap()
    }

    pub(crate) fn close(&self) {
        self.input.lock().unwrap().take();
    }

    pub(crate) fn join(&self) {
        self.close();
        if let Some(worker) = self.worker.lock().unwrap().take() {
            worker.join().unwrap();
        }
    }
}

struct Input {
    frames: Receiver<Vec<u8>>,
    current: Cursor<Vec<u8>>,
}

impl Read for Input {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        loop {
            let count = self.current.read(bytes)?;
            if count != 0 {
                return Ok(count);
            }
            match self.frames.recv() {
                Ok(frame) => self.current = Cursor::new(frame),
                Err(_) => return Ok(0),
            }
        }
    }
}

struct Output {
    frames: SyncSender<Vec<u8>>,
    current: Vec<u8>,
}

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        for &byte in bytes {
            if self.current.len() >= MAX_DIAGNOSTIC_PROTOCOL_FRAME_BYTES {
                return Err(io::ErrorKind::OutOfMemory.into());
            }
            self.current.push(byte);
            if byte == b'\n' {
                self.frames
                    .send(std::mem::take(&mut self.current))
                    .map_err(|_| io::ErrorKind::BrokenPipe)?;
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl DiagnosticTargetServer {
    pub(crate) fn test_stdio_endpoint() -> (Self, Arc<DiagnosticStdioClient>) {
        let (sender, receiver) = mpsc::sync_channel(DIAGNOSTIC_TARGET_REQUEST_QUEUE_CAPACITY);
        let terminal = Arc::new(AtomicBool::new(false));
        let wake = Arc::new(Mutex::new(None));
        let shell_sender = DiagnosticTargetShellRequestSender {
            sender,
            terminal: terminal.clone(),
            wake: wake.clone(),
            response_timeout: DIAGNOSTIC_TARGET_RESPONSE_TIMEOUT,
        };
        let (input, frames) = mpsc::sync_channel(1);
        let (responses, output) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("native-diagnostic-stdio".into())
            .spawn(move || {
                run_diagnostic_target_stdio_loop(
                    shell_sender,
                    Input {
                        frames,
                        current: Cursor::new(Vec::new()),
                    },
                    Output {
                        frames: responses,
                        current: Vec::new(),
                    },
                );
            })
            .unwrap();
        let client_terminal = terminal.clone();
        (
            Self {
                receiver,
                terminal,
                wake,
            },
            Arc::new(DiagnosticStdioClient {
                input: Mutex::new(Some(input)),
                output: Mutex::new(output),
                worker: Mutex::new(Some(worker)),
                terminal: client_terminal,
            }),
        )
    }
}
