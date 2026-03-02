use std::process::Command;
use std::time::{Duration, Instant};

use crate::ports::{ProcessOpts, ProcessResult, ProcessRunner};

pub struct OsProcess;

impl OsProcess {
    /// Send SIGTERM, wait up to `grace` for exit, then SIGKILL.
    #[cfg(unix)]
    fn kill_gracefully(
        child: &mut std::process::Child,
        grace: Duration,
    ) -> std::io::Result<()> {
        // SIGTERM
        unsafe {
            libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
        }
        let deadline = Instant::now() + grace;
        while Instant::now() < deadline {
            if child.try_wait()?.is_some() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        // Process didn't exit — SIGKILL
        child.kill()
    }
}

impl ProcessRunner for OsProcess {
    fn run(
        &self,
        cmd: &str,
        args: &[&str],
        opts: ProcessOpts,
    ) -> Result<ProcessResult, Box<dyn std::error::Error>> {
        let mut command = Command::new(cmd);
        command.args(args);

        if let Some(cwd) = &opts.cwd {
            command.current_dir(cwd);
        }

        for (k, v) in &opts.env {
            command.env(k, v);
        }

        let timeout = opts.timeout.unwrap_or(Duration::from_secs(300));
        let start = Instant::now();

        let mut child = command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        // Poll for completion, enforcing timeout
        loop {
            if start.elapsed() >= timeout {
                #[cfg(unix)]
                {
                    Self::kill_gracefully(&mut child, Duration::from_secs(5))?;
                }
                #[cfg(not(unix))]
                {
                    child.kill()?;
                }

                let output = child.wait_with_output()?;
                return Ok(ProcessResult {
                    success: false,
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                    exit_code: -1,
                    duration: start.elapsed(),
                });
            }

            match child.try_wait()? {
                Some(status) => {
                    let stdout = read_pipe(child.stdout.take());
                    let stderr = read_pipe(child.stderr.take());
                    let code = status.code().unwrap_or(-1);
                    return Ok(ProcessResult {
                        success: status.success(),
                        stdout,
                        stderr,
                        exit_code: code,
                        duration: start.elapsed(),
                    });
                }
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }
}

fn read_pipe(pipe: Option<impl std::io::Read>) -> String {
    match pipe {
        Some(mut r) => {
            let mut buf = String::new();
            let _ = std::io::Read::read_to_string(&mut r, &mut buf);
            buf
        }
        None => String::new(),
    }
}
