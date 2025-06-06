//! [![crates.io](https://img.shields.io/crates/v/karen?logo=rust)](https://crates.io/crates/karen/)
//! [![docs.rs](https://docs.rs/sudo/badge.svg)](https://docs.rs/sudo)
//!
//! Detect if you are running as root, restart self with `sudo` if needed or setup uid zero when running with the SUID flag set.
//!
//! ## Requirements
//!
//! * The `sudo` program is required to be installed and setup correctly on the target system.
//! * Linux or Mac OS X tested
//!     * It should work on *BSD. However, you can also create an Escalate builder with `doas` as the wrapper should you prefer that.
#![allow(clippy::bool_comparison)]

use std::error::Error;
use std::process::Command;

#[macro_use]
extern crate log;

/// Cross platform representation of the state the current program running
#[derive(Debug, PartialEq)]
pub enum RunningAs {
    /// Root (Linux/Mac OS/Unix) or Administrator (Windows)
    Root,
    /// Running as a normal user
    User,
    /// Started from SUID, a call to `karen::escalate_if_needed` or `karen::with_env` is required to claim the root privileges at runtime.
    /// This does not restart the process.
    Suid,
}
use RunningAs::*;

#[cfg(unix)]
/// Check getuid() and geteuid() to learn about the configuration this program is running under
pub fn check() -> RunningAs {
    let uid = unsafe { libc::getuid() };
    let euid = unsafe { libc::geteuid() };

    match (uid, euid) {
        (0, 0) => Root,
        (_, 0) => Suid,
        (_, _) => User,
    }
    //if uid == 0 { Root } else { User }
}

pub struct Escalate {
    wrapper: String,
}

impl Default for Escalate {
    fn default() -> Self {
        Escalate {
            wrapper: "sudo".to_string(),
        }
    }
}

impl Escalate {
    pub fn builder() -> Self {
        Default::default()
    }

    pub fn wrapper(&mut self, wrapper: &str) -> &mut Self {
        self.wrapper = wrapper.to_string();
        self
    }

    ///  Escalate privileges while maintaining RUST_BACKTRACE and selected environment variables (or none).
    ///
    /// Activates SUID privileges when available.
    pub fn with_env(&self, prefixes: &[&str]) -> Result<RunningAs, Box<dyn Error>> {
        let current = check();
        trace!("Running as {:?}", current);
        match current {
            Root => {
                trace!("already running as Root");
                return Ok(current);
            }
            Suid => {
                trace!("setuid(0)");
                unsafe {
                    libc::setuid(0);
                }
                return Ok(current);
            }
            User => {
                debug!("Escalating privileges");
            }
        }

        let mut args: Vec<_> = std::env::args().collect();
        if let Some(absolute_path) = std::env::current_exe()
            .ok()
            .and_then(|p| p.to_str().map(|p| p.to_string()))
        {
            args[0] = absolute_path;
        }
        let mut command: Command = Command::new(&self.wrapper);

        // Always propagate RUST_BACKTRACE
        if let Ok(trace) = std::env::var("RUST_BACKTRACE") {
            let value = match &*trace.to_lowercase() {
                "" => None,
                "1" | "true" => Some("1"),
                "full" => Some("full"),
                invalid => {
                    warn!(
                        "RUST_BACKTRACE has invalid value {:?} -> defaulting to \"full\"",
                        invalid
                    );
                    Some("full")
                }
            };
            if let Some(value) = value {
                trace!("relaying RUST_BACKTRACE={}", value);
                // command.arg(format!("RUST_BACKTRACE={}", value));
                command.env("RUST_BACKTRACE", value);
            }
        }

        if !prefixes.is_empty() {
            // Only add env for pkexec if we're passing any additional env vars
            if self.wrapper == "pkexec" {
                trace!("Prefixing `env` to pkexec command to pass additional environment variables! This may break pkexec system policies.");
                command.arg("env");
            }
            for (name, value) in std::env::vars().filter(|(name, _)| name != "RUST_BACKTRACE") {
                if prefixes.iter().any(|prefix| name.starts_with(prefix)) {
                    trace!("propagating {}={}", name, value);
                    if self.wrapper == "pkexec" {
                        command.arg(format!("{}={}", name, value));
                    }
                    command.env(name, value);
                }
            }
        }

        let mut child = command.args(args).spawn().expect("failed to execute child");

        let ecode = child.wait().expect("failed to wait on child");

        if ecode.success() == false {
            std::process::exit(ecode.code().unwrap_or(1));
        } else {
            std::process::exit(0);
        }
    }

    /// Restart your program with root privileges if the user is not privileged enough.
    ///
    /// Activates SUID privileges when available
    pub fn escalate_if_needed(&self) -> Result<RunningAs, Box<dyn Error>> {
        self.with_env(&[])
    }
}

#[cfg(unix)]
/// Alias for Escalate::builder() to quickly create a new karen Escalate builder
pub fn builder() -> Escalate {
    Escalate::builder()
}

#[cfg(unix)]
/// Restart your program with sudo if the user is not privileged enough.
///
/// Activates SUID privileges when available
///
/// ```
/// # use std::error::Error;
/// # fn main() -> Result<(), Box<dyn Error>> {
/// #   if karen::check() == karen::RunningAs::Root {
/// karen::escalate_if_needed()?;
/// // the following gets only executed in privileged mode
/// #   } else {
/// #     eprintln!("not actually testing");
/// #   }
/// #   Ok(())
/// # }
/// ```
#[inline]
pub fn escalate_if_needed() -> Result<RunningAs, Box<dyn Error>> {
    with_env(&[])
}

#[cfg(unix)]
/// Similar to escalate_if_needed, but with pkexec as the wrapper
///
/// ```
/// # use std::error::Error;
/// # fn main() -> Result<(), Box<dyn Error>> {
/// #   if karen::check() == karen::RunningAs::Root {
/// karen::pkexec()?;
/// // the following gets only executed in privileged mode
/// #   } else {
/// #     eprintln!("not actually testing");
/// #   }
/// #   Ok(())
/// # }
/// ```
#[inline]
pub fn pkexec() -> Result<RunningAs, Box<dyn Error>> {
    builder().wrapper("pkexec").escalate_if_needed()
}

#[cfg(unix)]
/// Similar to with_env, but with pkexec as the wrapper
///
/// ```
/// # use std::error::Error;
/// # fn main() -> Result<(), Box<dyn Error>> {
/// #   if karen::check() == karen::RunningAs::Root {
/// karen::pkexec_with_env(&["CARGO_", "MY_APP_"])?;
/// // the following gets only executed in privileged mode
/// #   } else {
/// #     eprintln!("not actually testing");
/// #   }
/// #   Ok(())
/// # }
///
#[inline]
pub fn pkexec_with_env(prefixes: &[&str]) -> Result<RunningAs, Box<dyn Error>> {
    builder().wrapper("pkexec").with_env(prefixes)
}

#[cfg(unix)]
/// Similar to escalate_if_needed, but with doas as the wrapper
///
/// ```
/// # use std::error::Error;
/// # fn main() -> Result<(), Box<dyn Error>> {
/// #   if karen::check() == karen::RunningAs::Root {
/// karen::doas()?;
/// // the following gets only executed in privileged mode
/// #   } else {
/// #     eprintln!("not actually testing");
/// #   }
/// #   Ok(())
/// # }
/// ```
#[inline]
pub fn doas() -> Result<RunningAs, Box<dyn Error>> {
    builder().wrapper("doas").escalate_if_needed()
}

#[cfg(unix)]
/// Escalate privileges while maintaining RUST_BACKTRACE and selected environment variables (or none).
///
/// Activates SUID privileges when available.
///
/// ```
/// # use std::error::Error;
/// # fn main() -> Result<(), Box<dyn Error>> {
/// #   if karen::check() == karen::RunningAs::Root {
/// karen::with_env(&["CARGO_", "MY_APP_"])?;
/// // the following gets only executed in privileged mode
/// #   } else {
/// #     eprintln!("not actually testing");
/// #   }
/// #   Ok(())
/// # }
/// ```
pub fn with_env(prefixes: &[&str]) -> Result<RunningAs, Box<dyn Error>> {
    Escalate::default().with_env(prefixes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let c = check();
        assert!(true, "{:?}", c);
    }
}
