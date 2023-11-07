use crate::api::action::Action;
use crate::core::logger::Logger;
use crate::monitor::Monitor;
use std::borrow::Cow;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::exit;
use std::sync::{Arc, Mutex};

pub mod action;

pub const UNIX_DOMAIN_SOCKET_PATH: &str = "/tmp/.unixdomain.sock";

pub struct Responder {
    logger: Arc<Mutex<Logger>>,
    monitor: Monitor,
}

impl Responder {
    fn bind_listener(&self) -> UnixListener {
        let mut logger = self.logger.lock().unwrap();
        return match UnixListener::bind(UNIX_DOMAIN_SOCKET_PATH) {
            Ok(stream) => {
                logger.resp_log(format!(
                    "Socket was successfully created: {UNIX_DOMAIN_SOCKET_PATH}"
                ));
                stream
            }
            Err(_) => {
                logger.log_err(format!(
                    "Error! Can't bind socket \"{UNIX_DOMAIN_SOCKET_PATH}\""
                ));
                exit(2);
            }
        };
    }

    fn write_message(&mut self, mut stream: UnixStream, message: String) {
        let mut logger = self.logger.lock().unwrap();
        if let Err(e) = stream.write(message.as_bytes()) {
            logger.resp_log(format!(
                "Error! Can't answer to the client with message: \"{message}\": {e}"
            ));
        } else {
            logger.resp_log(format!("Sending the answer: \"{message}\""));
        }
        if let Err(e) = stream.flush() {
            logger.resp_log(format!("Error! Can't flush the stdout: {e}"));
        }
    }

    fn handle_message(&mut self, stream: UnixStream, received_data: Cow<str>) {
        {
            let mut logger = self.logger.lock().unwrap();
            logger.resp_log(format!("Received via socket: {received_data}"));
        }
        match serde_json::from_str::<Action>(received_data.to_string().as_str()) {
            Ok(action) => {
                let answer = self.monitor.answer(action);
                self.write_message(stream, answer);
            }
            Err(error) => {
                {
                    let mut logger = self.logger.lock().unwrap();
                    logger.resp_log(format!("Error! Unknown action: {received_data}: {error}"));
                }
                self.write_message(stream, "Error! Unknown action".to_string());
            }
        }
    }

    pub fn listen(monitor: Monitor, logger: Arc<Mutex<Logger>>) {
        let mut responder = Responder { logger, monitor };
        for stream in responder.bind_listener().incoming() {
            match stream {
                Ok(mut stream) => {
                    let mut buffer = [0; 1024];
                    match stream.read(&mut buffer) {
                        Ok(bytes_read) => {
                            if bytes_read == 0 {
                                continue;
                            }
                            responder.handle_message(
                                stream,
                                String::from_utf8_lossy(&buffer[..bytes_read]),
                            );
                        }
                        Err(e) => {
                            let mut logger = responder.logger.lock().unwrap();
                            logger.resp_log(format!("Stream: {e}"));
                        }
                    }
                }
                Err(e) => {
                    let logger = responder.logger.lock().unwrap();
                    logger.log_err(format!("Error! Can't accept a connection: {e}"));
                }
            }
        }
    }
}
