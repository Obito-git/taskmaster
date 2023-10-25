pub mod api;
pub mod data;

use crate::data::Configuration;
use crate::data::State;
use crate::data::State::{FATAL, REGISTERED, STARTING};
use std::fmt::{Display, Formatter};
use std::fs::{File, OpenOptions};
use std::process::{Child, Command, Stdio};
use crate::api::error_log::ErrorLog;

pub const UNIX_DOMAIN_SOCKET_PATH: &str = "/tmp/.unixdomain.sock";

//TODO: Validation of stdout/stderr files path
//TODO: Check existing of working dir

pub struct Task {
    configuration: Configuration,
    state: State,
    _restarts_left: u32,
    child: Option<Child>,
    _started_at: &'static str,
    logger: ErrorLog,
}

impl Task {
    pub fn new(configuration: Configuration) -> Task {
        Task {
            _restarts_left: configuration.start_retries,
            configuration,
            state: REGISTERED,
            child: None,
            _started_at: "time",
            logger: ErrorLog::new(),
        }
    }

    fn open_file(path: &String) -> Result<File, String> {
        OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .map_err(|e| e.to_string())
    }


    fn setup_stream(&self, stream_type: &Option<String>) -> Result<Stdio, String> {
        match stream_type {
            Some(path) => Task::open_file(path).map(|file| file.into()),
            None => Ok(Stdio::null()),
        }
    }


    fn setup_child_process(&mut self, stderr: Stdio, stdout: Stdio) -> Result<(), String> {
        let argv: Vec<_> = self.configuration.cmd.split_whitespace().collect();

        match Command::new(argv[0])
            .args(&argv[1..])
            .current_dir(match &self.configuration.working_dir {
                Some(cwd) => &cwd,
                None => ".",
            })
            .envs(&self.configuration.env)
            .stdout(stdout)
            .stderr(stderr)
            .spawn() {
            Ok(child) => {
                self.child = Some(child);
                self.state = STARTING;
                Ok(())
            }
            Err(err) => {
                let err_msg = self.logger.log(format!("{err}").as_str(), None);
                println!("{}", err_msg);

                self.state = FATAL;

                Err(err_msg.to_string())
            }
        }
    }

    pub fn run(&mut self) -> Result<(), String> {
        let stderr = self.setup_stream(&self.configuration.stderr)
            .map_err(|e| {
                self.state = FATAL;
                self.logger.log(e.as_str(), None).to_string();
                e
            })?;
        let stdout = self.setup_stream(&self.configuration.stdout)
            .map_err(|e| {
                self.state = FATAL;
                self.logger.log(e.as_str(), None).to_string();
                e
            })?;

        self.setup_child_process(stderr, stdout)?;

        Ok(())
    }


    pub fn stop(&mut self) {}

    pub fn get_state(&self) -> &State {
        &self.state
    }

    pub fn get_json_configuration(&self) -> String {
        serde_json::to_string_pretty(&self.configuration).expect("Serialization failed")
    }
}

impl Display for Task {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.state)
    }
}
