use std::collections::HashMap;
use std::process::{Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub working_dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutorError {
    Spawn(String),
    Failed(CommandResult),
    Timeout,
}

pub trait CommandExecutor: Send + Sync {
    fn run(&self, command: &CommandSpec, timeout: Duration)
        -> Result<CommandResult, ExecutorError>;
}

#[derive(Debug, Clone, Default)]
pub struct LocalExecutor;

impl CommandExecutor for LocalExecutor {
    fn run(
        &self,
        command: &CommandSpec,
        timeout: Duration,
    ) -> Result<CommandResult, ExecutorError> {
        let mut process = std::process::Command::new(&command.program);
        process.args(&command.args);
        process.stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some(dir) = &command.working_dir {
            process.current_dir(dir);
        }
        let child = process
            .spawn()
            .map_err(|error| ExecutorError::Spawn(error.to_string()))?;
        let output = wait_with_timeout(child, timeout)?;
        let result = command_result(output);
        if result.status == 0 {
            Ok(result)
        } else {
            Err(ExecutorError::Failed(result))
        }
    }
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Result<Output, ExecutorError> {
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|error| ExecutorError::Spawn(error.to_string()))
            }
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ExecutorError::Timeout);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => return Err(ExecutorError::Spawn(error.to_string())),
        }
    }
}

fn command_result(output: Output) -> CommandResult {
    CommandResult {
        status: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostCheck {
    Ready,
    MissingProgram(String),
    Unavailable { program: String, detail: String },
}

pub fn validate_local_host(executor: &dyn CommandExecutor) -> Vec<HostCheck> {
    ["docker", "bash"]
        .iter()
        .map(|program| {
            let command = CommandSpec {
                program: (*program).into(),
                args: vec!["--version".into()],
                working_dir: None,
            };
            match executor.run(&command, Duration::from_secs(3)) {
                Ok(_) => HostCheck::Ready,
                Err(ExecutorError::Spawn(_)) => HostCheck::MissingProgram((*program).into()),
                Err(error) => HostCheck::Unavailable {
                    program: (*program).into(),
                    detail: format!("{error:?}"),
                },
            }
        })
        .collect()
}

#[derive(Clone, Default)]
pub struct FakeExecutor {
    responses: Arc<Mutex<HashMap<String, Result<CommandResult, ExecutorError>>>>,
    pub calls: Arc<Mutex<Vec<CommandSpec>>>,
}

impl FakeExecutor {
    pub fn respond(&self, program: &str, result: Result<CommandResult, ExecutorError>) {
        self.responses
            .lock()
            .unwrap()
            .insert(program.into(), result);
    }
}

impl CommandExecutor for FakeExecutor {
    fn run(
        &self,
        command: &CommandSpec,
        _timeout: Duration,
    ) -> Result<CommandResult, ExecutorError> {
        self.calls.lock().unwrap().push(command.clone());
        self.responses
            .lock()
            .unwrap()
            .get(&command.program)
            .cloned()
            .unwrap_or_else(|| Err(ExecutorError::Spawn("no fake response".into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_executor_does_not_depend_on_cwd_when_working_dir_is_explicit() {
        let result = LocalExecutor
            .run(
                &CommandSpec {
                    program: "printf".into(),
                    args: vec!["ok".into()],
                    working_dir: Some("/".into()),
                },
                Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(result.stdout, "ok");
    }

    #[test]
    fn fake_executor_records_commands_and_reports_failures() {
        let executor = FakeExecutor::default();
        executor.respond(
            "docker",
            Err(ExecutorError::Spawn("daemon unavailable".into())),
        );
        let checks = validate_local_host(&executor);
        assert!(matches!(
            checks[0],
            HostCheck::MissingProgram(_) | HostCheck::Unavailable { .. }
        ));
        assert_eq!(executor.calls.lock().unwrap()[0].program, "docker");
    }
}
