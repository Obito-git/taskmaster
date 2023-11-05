use crate::core::configuration::StopSignal::TERM;
use crate::core::logger::Logger;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use validator::{Validate, ValidationError};

#[derive(Debug, Eq, PartialEq, Deserialize, Serialize, Clone)]
pub enum AutoRestart {
    #[serde(rename = "true")]
    True,
    #[serde(rename = "false")]
    False,
    #[serde(rename = "unexpected")]
    Unexpected,
}

#[derive(Debug, Eq, PartialEq, Deserialize, Serialize, Clone)]
pub enum StopSignal {
    TERM,
    HUP,
    INT,
    QUIT,
    KILL,
    USR1,
    USR2,
    OTHER(String),
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum State {
    FINISHED, // TODO: delete, here to debug
    STOPPED(Option<SystemTime>),
    STARTING,
    RUNNING(SystemTime),
    BACKOFF,
    _EXITED,
    FATAL(String),
    _UNKNOWN,
}

impl Display for StopSignal {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TERM => write!(f, "TERM"),
            StopSignal::HUP => write!(f, "HUP"),
            StopSignal::INT => write!(f, "INT"),
            StopSignal::QUIT => write!(f, "QUIT"),
            StopSignal::KILL => write!(f, "KILL"),
            StopSignal::USR1 => write!(f, "USR1"),
            StopSignal::USR2 => write!(f, "USR2"),
            StopSignal::OTHER(custom) => write!(f, "{custom}"),
        }
    }
}

impl Display for State {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let keyword = match self {
            State::FINISHED => "finished".to_string(),
            State::STOPPED(stopped_at) => match stopped_at {
                None => "stopped".to_string(),
                Some(stopped_at) => {
                    let since_the_epoch = stopped_at.duration_since(UNIX_EPOCH).unwrap();
                    let now_in_sec = since_the_epoch.as_secs();
                    let hours = (now_in_sec % (24 * 3600)) / 3600;
                    let minutes = (now_in_sec % 3600) / 60;
                    let seconds = now_in_sec % 60;
                    format!("stopped\tat {:02}:{:02}:{:02}", hours, minutes, seconds)
                }
            },
            State::STARTING => "starting".to_string(),
            State::RUNNING(start_time) => {
                let current_time = SystemTime::now();
                let elapsed_time = current_time
                    .duration_since(start_time.clone())
                    .unwrap_or(Duration::from_secs(0));
                let elapsed_time_seconds = elapsed_time.as_secs();
                let hours = elapsed_time_seconds / 3600;
                let minutes = (elapsed_time_seconds % 3600) / 60;
                let seconds = elapsed_time_seconds % 60;
                format!("running\tuptime {:02}:{:02}:{:02}", hours, minutes, seconds)
            }
            State::BACKOFF => "backoff".to_string(),
            State::_EXITED => "exited".to_string(),
            State::FATAL(error) => {
                format!("fatal\t{error}")
            }
            State::_UNKNOWN => "unknown".to_string(),
        };
        write!(f, "{}", keyword)
    }
}

#[derive(Debug, Eq, PartialEq, Deserialize, Serialize, Clone, Validate)]
#[serde(default)]
pub struct Configuration {
    #[serde(deserialize_with = "deserialize_string_and_trim")]
    #[validate(length(min = 1, message = "cmd: can't be empty"))]
    pub cmd: String, //make immutable (e.g. getters?)
    #[validate(range(min = 1))]
    pub num_procs: u32,
    #[serde(deserialize_with = "deserialize_umask")]
    #[validate(custom = "validate_umask")]
    pub umask: u32,
    #[serde(deserialize_with = "deserialize_option_string_and_trim")]
    pub working_dir: Option<String>,
    pub auto_start: bool,
    pub auto_restart: AutoRestart,
    exit_codes: Vec<i32>,
    pub start_retries: u32, //make immutable (e.g. getters?)
    pub start_time: u64,
    pub stop_signal: StopSignal,
    stop_time: u32,
    #[serde(deserialize_with = "deserialize_option_string_and_trim")]
    pub stdout: Option<String>,
    #[serde(deserialize_with = "deserialize_option_string_and_trim")]
    pub stderr: Option<String>,
    pub env: BTreeMap<String, String>,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            cmd: String::new(),
            num_procs: 1,
            umask: 0o022,
            working_dir: None,
            auto_start: true,
            auto_restart: AutoRestart::Unexpected,
            exit_codes: vec![0],
            start_retries: 3,
            start_time: 1,
            stop_signal: TERM,
            stop_time: 10,
            stdout: None,
            stderr: None,
            env: Default::default(),
        }
    }
}

impl Configuration {
    pub fn from_yml(path: String) -> Result<BTreeMap<String, Configuration>, String> {
        let logger = Logger::new(None);
        logger.log(format!("Reading {path}"));
        let mut file = File::open(&path).map_err(|err| format!("{}: {}", path, err))?;
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|err| format!("Can't read the file: {err}"))?;
        let tasks: BTreeMap<String, Configuration> =
            serde_yaml::from_str(&content).map_err(|err| err.to_string())?;
        for (key, task) in &tasks {
            match task.validate() {
                Ok(_) => logger.log(format!("{key}: validated")),
                Err(e) => {
                    for (_k, value) in e.field_errors() {
                        return if let Some(message) = value[0].message.as_ref() {
                            Err(format!("{key}: {:?}", message.to_string()))
                        } else {
                            Err(format!("{key}: {:?}", value[0].code))
                        };
                    }
                }
            }
        }
        Ok(tasks)
    }
}

fn validate_umask(value: u32) -> Result<(), ValidationError> {
    if !(value & 0o777 == value) {
        return Err(ValidationError::new("Invalid umask"));
    }
    Ok(())
}

fn deserialize_umask<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    match u32::from_str_radix(value.as_str(), 8) {
        Ok(umask) => Ok(umask),
        Err(_) => Err(serde::de::Error::custom(format!(
            "\"{value}\" is not a valid umask."
        ))),
    }
}

fn deserialize_string_and_trim<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    String::deserialize(deserializer).map(|s| s.trim().to_string())
}

fn deserialize_option_string_and_trim<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    String::deserialize(deserializer).map(|s| Some(s.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use crate::core::configuration::AutoRestart::Unexpected;
    use crate::core::configuration::Configuration;
    use crate::core::configuration::StopSignal::TERM;
    use std::collections::BTreeMap;

    const CMD_EMPTY: &str = "config_files/test/cmd_empty.yml";
    const CMD_NOT_PROVIDED: &str = "config_files/test/cmd_not_provided.yml";
    const CMD_ONLY_WHITE_SPACES: &str = "config_files/test/cmd_white_spaces.yml";
    const CMD_WHITE_SPACES_BEFORE_AND_AFTER: &str =
        "config_files/test/cmd_whitespaces_before_and_after.yml";
    const CONFIG_ONLY_CMD_PRESENT: &str = "config_files/test/config_only_cmd.yml";
    const MASK_NOT_VALID_1: &str = "config_files/test/umask_not_valid_1.yml";
    const MASK_NOT_VALID_2: &str = "config_files/test/umask_not_valid_2.yml";
    const WORKING_DIR_WITH_SPACES: &str = "config_files/test/working_dir_with_spaces.yml";
    const STDOUT_WITH_SPACES: &str = "config_files/test/stdout_path_with_spaces.yml";
    const STDERR_WITH_SPACES: &str = "config_files/test/stderr_path_with_spaces.yml";

    #[test]
    fn cmd_empty_should_return_error() {
        //given && when
        let task = Configuration::from_yml(CMD_EMPTY.into());

        //then
        assert!(task.is_err());
        if let Err(error) = task {
            assert_eq!("task1: \"cmd: can't be empty\"", error.to_string())
        }
    }

    #[test]
    fn cmd_not_provided_should_return_error() {
        //given && when
        let task = Configuration::from_yml(CMD_NOT_PROVIDED.into());

        //then
        assert!(task.is_err());
        if let Err(error) = task {
            assert_eq!("task1: \"cmd: can't be empty\"", error.to_string())
        }
    }

    #[test]
    fn cmd_white_spaces_should_return_error() {
        //given && when
        let task = Configuration::from_yml(CMD_ONLY_WHITE_SPACES.into());

        //then
        assert!(task.is_err());
        if let Err(error) = task {
            assert_eq!("task1: \"cmd: can't be empty\"", error.to_string())
        }
    }

    #[test]
    fn cmd_white_spaces_before_and_after_should_return_trimmed_cmd() {
        //given
        let expected_key = String::from("task1");
        let expected_value = Configuration {
            cmd: String::from("while true; do echo 'Task 1 output'; sleep 3; done"),
            num_procs: 1,
            umask: 0o777,
            working_dir: Some(String::from("/tmp")),
            auto_start: true,
            auto_restart: Unexpected,
            exit_codes: vec![0, 2],
            start_retries: 3,
            start_time: 5,
            stop_signal: TERM,
            stop_time: 10,
            stdout: Some(String::from("/tmp/task1.stdout")),
            stderr: Some(String::from("/tmp/task1.stderr")),
            env: BTreeMap::new(),
        };

        // when
        let task = Configuration::from_yml(CMD_WHITE_SPACES_BEFORE_AND_AFTER.into()).unwrap();

        //then
        let mut expected_map = BTreeMap::new();
        expected_map.insert(expected_key, expected_value);
        assert_eq!(task, expected_map);
    }

    #[test]
    fn only_cmd_cfg_should_put_default_values() {
        //given
        let mut expected_task = Configuration::default();
        expected_task.cmd = String::from("only_cmd_is_present");
        let expected_key = String::from("task1");
        let mut expected: BTreeMap<String, Configuration> = BTreeMap::new();
        expected.insert(expected_key, expected_task);

        // when
        let task = Configuration::from_yml(CONFIG_ONLY_CMD_PRESENT.into()).unwrap();

        //then
        assert_eq!(expected, task);
    }

    #[test]
    fn umask_unvalid_1_should_return_error() {
        //given && when
        let task = Configuration::from_yml(MASK_NOT_VALID_1.into());

        //then
        assert!(task.is_err());
        if let Err(error) = task {
            assert_eq!(error.to_string(), "task1: \"Invalid umask\"");
        }
    }

    #[test]
    fn umask_unvalid_2_should_return_error() {
        //given && when
        let task = Configuration::from_yml(MASK_NOT_VALID_2.into());

        //then
        assert!(task.is_err());
        if let Err(error) = task {
            assert!(error.to_string().contains("is not a valid umask"));
        }
    }

    #[test]
    fn working_dir_with_spaces_should_be_trimmed() {
        //given
        let mut expected_task = Configuration::default();
        expected_task.cmd = String::from("cmd");
        expected_task.working_dir = Some(String::from("/tmp"));
        let expected_key = String::from("task1");
        let mut expected: BTreeMap<String, Configuration> = BTreeMap::new();
        expected.insert(expected_key, expected_task);

        // when
        let task = Configuration::from_yml(WORKING_DIR_WITH_SPACES.into()).unwrap();

        //then
        assert_eq!(expected, task);
    }

    #[test]
    fn stdout_path_with_spaces_should_be_trimmed() {
        //given
        let mut expected_task = Configuration::default();
        expected_task.cmd = String::from("cmd");
        expected_task.stdout = Some(String::from("/tmp/task1.stdout"));
        let expected_key = String::from("task1");
        let mut expected: BTreeMap<String, Configuration> = BTreeMap::new();
        expected.insert(expected_key, expected_task);

        // when
        let task = Configuration::from_yml(STDOUT_WITH_SPACES.into()).unwrap();

        //then
        assert_eq!(expected, task);
    }

    #[test]
    fn stderr_path_with_spaces_should_be_trimmed() {
        //given
        let mut expected_task = Configuration::default();
        expected_task.cmd = String::from("cmd");
        expected_task.stderr = Some(String::from("/tmp/task1.stderr"));
        let expected_key = String::from("task1");
        let mut expected: BTreeMap<String, Configuration> = BTreeMap::new();
        expected.insert(expected_key, expected_task);

        // when
        let task = Configuration::from_yml(STDERR_WITH_SPACES.into()).unwrap();

        //then
        assert_eq!(expected, task);
    }
}
