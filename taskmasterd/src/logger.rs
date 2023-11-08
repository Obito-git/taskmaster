use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::net::TcpStream;
use std::time::{SystemTime, UNIX_EPOCH};

const MONITOR_THREAD_PREFIX: &'static str = "MONITOR THREAD";
const MONITOR_PREFIX: &'static str = "    MONITOR   ";
const RESPONDER_PREFIX: &'static str = "   RESPONDER  ";
const GLOBAL_PREFIX: &'static str = "  RUSTMASTER  ";
const HTTP_LOGGER_PREFIX: &'static str = " HTTP_LOGGER  ";
const MAX_MESSAGES: usize = 1000;
const BUFFER_SIZE: usize = MAX_MESSAGES * 6 / 5;

const URL_ADDR: &'static str = "127.0.0.1";
const URL_PORT: usize = 4242;

pub type LogLine = (usize, String);

pub struct Logger {
    pub history: VecDeque<LogLine>,
    file: File,
    idx: usize,
    http_log_stream: Option<TcpStream>,
}

impl Logger {
    fn get_timestamp() -> String {
        let now = SystemTime::now();
        let since_the_epoch = now.duration_since(UNIX_EPOCH).unwrap();
        let now_in_sec = since_the_epoch.as_secs();
        let hours = (now_in_sec % (24 * 3600)) / 3600;
        let minutes = (now_in_sec % 3600) / 60;
        let seconds = now_in_sec % 60;
        format!("[{:02}:{:02}:{:02}]: ", hours, minutes, seconds)
    }

    pub fn get_history(&self, num_lines: Option<usize>) -> Vec<String> {
        self.history
            .iter()
            .rev()
            .take(num_lines.unwrap_or(self.history.len()))
            .rev()
            .map(|(_, message)| message.to_string())
            .collect()
    }

    pub fn new(file_path: &'static str) -> Result<Self, String> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(file_path)
            .map_err(|e| format!("Can't create logging file: {file_path}. Error: {e}"))?;
        Ok(Logger {
            history: VecDeque::with_capacity(BUFFER_SIZE),
            file,
            idx: 0,
            http_log_stream: None,
        })
    }

    pub fn enable_http_logging(&mut self, port: u16) -> Result<(), String> {
        if let Some(_) = self.http_log_stream {
            return Err("Http logging is already enabled".to_string());
        }
        let stream = TcpStream::connect(format!("{}:{}", "localhost", port))
            .map_err(|e| format!("Can't connect to localhost:{port}: {e}"))?;
        self.do_log(
            HTTP_LOGGER_PREFIX,
            format!("Connection with localhost:{port} has been established").as_str(),
        );
        self.http_log_stream = Some(stream);
        Ok(())
    }

    pub fn disable_http_logging(&mut self) -> String {
        if self.http_log_stream.is_none() {
            return "Http logging is already disabled".to_string();
        }
        self.http_log_stream = None;
        self.do_log(
            HTTP_LOGGER_PREFIX,
            format!("Http logging was disabled by client").as_str(),
        );
        format!("Http logging has been disabled")
    }

    fn http_logging(&mut self, body: &str) {
        if let Some(stream) = &mut self.http_log_stream {
            let request = format!(
                "POST / HTTP/1.1\r\n\
         Content-Type: application/x-www-form-urlencoded\r\n\
         Content-Length: {}\r\n\
         \r\n\
         {}",
                body.len(),
                body
            );

            if let Err(err) = stream.write_all(request.as_bytes()) {
                self.do_log(
                    HTTP_LOGGER_PREFIX,
                    format!("Can't write log via http: {err}, disabling...").as_str(),
                );
                self.http_log_stream = None
            }
        }
    }

    fn do_log(&mut self, prefix: &'static str, message: &str) {
        let log_msg = format!(
            "[{prefix}]: {}{:?}\n",
            Logger::get_timestamp(),
            message.trim()
        );
        print!("{log_msg}");
        if let Err(e) = self.file.write_all(log_msg.as_bytes()) {
            eprintln!("Error! Can't write log {message} in log file: {e}")
        }
        if prefix != RESPONDER_PREFIX {
            self.idx = self.idx.wrapping_add(1);
            self.history.push_back((self.idx, log_msg));
            if self.history.len() > (BUFFER_SIZE as f32 * 0.95) as usize {
                self.history.drain(..(self.history.len() - MAX_MESSAGES));
            }
        }
        if prefix != HTTP_LOGGER_PREFIX {
            self.http_logging(message);
        }
    }

    pub fn sth_log(&mut self, message: String) -> String {
        self.do_log(MONITOR_THREAD_PREFIX, &message);
        message
    }

    pub fn monit_log(&mut self, message: String) -> String {
        self.do_log(MONITOR_PREFIX, &message);
        message
    }

    pub fn log<S: AsRef<str>>(&mut self, message: S) {
        self.do_log(GLOBAL_PREFIX, message.as_ref());
    }

    pub fn resp_log<S: AsRef<str>>(&mut self, message: S) {
        self.do_log(RESPONDER_PREFIX, message.as_ref());
    }

    pub fn log_err<S: AsRef<str>>(&self, message: S) {
        eprintln!("{}", message.as_ref())
    }
}
