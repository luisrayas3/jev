use std::collections::VecDeque;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

use anyhow::{bail, Result};
use linkme::distributed_slice;

// -- Policy / Decision -------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Policy {
    Allow = 1,
    Prompt = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Prompt,
    Reject,
}

// -- CrossingInfo ------------------------------------------------------------

pub struct CrossingInfo {
    pub file: &'static str,
    pub line: u32,
    pub kind: &'static str,
    pub detail: &'static str,
    policy: AtomicU8,
}

unsafe impl Sync for CrossingInfo {}

impl CrossingInfo {
    pub const fn new(
        file: &'static str,
        line: u32,
        kind: &'static str,
        detail: &'static str,
    ) -> Self {
        CrossingInfo {
            file,
            line,
            kind,
            detail,
            policy: AtomicU8::new(0),
        }
    }

    pub fn policy(&self) -> Option<Policy> {
        match self.policy.load(Ordering::Acquire) {
            1 => Some(Policy::Allow),
            2 => Some(Policy::Prompt),
            _ => None,
        }
    }

    pub fn set_policy(&self, p: Policy) {
        self.policy.store(p as u8, Ordering::Release);
    }
}

// -- Distributed slice -------------------------------------------------------

#[distributed_slice]
pub static CROSSINGS: [CrossingInfo];

// -- Test injection ----------------------------------------------------------

static INJECTED_DECISIONS: Mutex<VecDeque<Decision>> =
    Mutex::new(VecDeque::new());
static INJECTED_RESPONSES: Mutex<VecDeque<bool>> =
    Mutex::new(VecDeque::new());

pub fn inject_decision(decision: Decision) {
    INJECTED_DECISIONS.lock().unwrap().push_back(decision);
}

pub fn inject_response(approve: bool) {
    INJECTED_RESPONSES.lock().unwrap().push_back(approve);
}

// -- Init: collect decisions for all crossings --------------------------------

pub fn init() -> Result<()> {
    if CROSSINGS.is_empty() {
        return Ok(());
    }

    eprintln!("Label crossings:");
    for crossing in CROSSINGS.iter() {
        let label = if crossing.detail.is_empty() {
            format!(
                "  L{}: {}",
                crossing.line, crossing.kind,
            )
        } else {
            format!(
                "  L{}: {} {}",
                crossing.line, crossing.kind, crossing.detail,
            )
        };

        let decision = pop_injected_decision()
            .map(Ok)
            .unwrap_or_else(|| prompt_decision(&label))?;

        match decision {
            Decision::Reject => bail!(
                "Rejected crossing at {}:{}",
                crossing.file, crossing.line,
            ),
            Decision::Allow => crossing.set_policy(Policy::Allow),
            Decision::Prompt => crossing.set_policy(Policy::Prompt),
        }
    }
    Ok(())
}

fn pop_injected_decision() -> Option<Decision> {
    INJECTED_DECISIONS.lock().unwrap().pop_front()
}

fn prompt_decision(label: &str) -> Result<Decision> {
    let stderr = std::io::stderr();
    let mut err = stderr.lock();
    write!(err, "{label} [a]llow / [p]rompt / [r]eject: ")?;
    err.flush()?;

    let stdin = std::io::stdin();
    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;

    match line.trim() {
        "a" | "A" => Ok(Decision::Allow),
        "p" | "P" => Ok(Decision::Prompt),
        _ => Ok(Decision::Reject),
    }
}

// -- Check: enforce policy at a crossing site --------------------------------

pub fn check(info: &CrossingInfo) -> Result<()> {
    match info.policy() {
        Some(Policy::Allow) => Ok(()),
        Some(Policy::Prompt) => {
            if let Some(approve) = pop_injected_response() {
                if approve {
                    Ok(())
                } else {
                    bail!(
                        "Denied crossing at {}:{}",
                        info.file, info.line,
                    )
                }
            } else {
                prompt_runtime(info)
            }
        }
        None => bail!(
            "No policy set for crossing at {}:{}",
            info.file, info.line,
        ),
    }
}

fn pop_injected_response() -> Option<bool> {
    INJECTED_RESPONSES.lock().unwrap().pop_front()
}

fn prompt_runtime(info: &CrossingInfo) -> Result<()> {
    let stderr = std::io::stderr();
    let mut err = stderr.lock();

    let label = if info.detail.is_empty() {
        format!(
            "L{}: {}",
            info.line, info.kind,
        )
    } else {
        format!(
            "L{}: {} {}",
            info.line, info.kind, info.detail,
        )
    };

    write!(err, "  {label} — approve? [y/N]: ")?;
    err.flush()?;

    let stdin = std::io::stdin();
    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;

    match line.trim() {
        "y" | "Y" | "yes" => Ok(()),
        _ => bail!(
            "Denied crossing at {}:{}",
            info.file, info.line,
        ),
    }
}

// -- Tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossing_info_new_and_policy() {
        let info = CrossingInfo::new("test.rs", 10, "declassify", "");
        assert_eq!(info.policy(), None);
        info.set_policy(Policy::Allow);
        assert_eq!(info.policy(), Some(Policy::Allow));
        info.set_policy(Policy::Prompt);
        assert_eq!(info.policy(), Some(Policy::Prompt));
    }

    #[test]
    fn check_allow_passes() {
        let info = CrossingInfo::new("test.rs", 1, "declassify", "");
        info.set_policy(Policy::Allow);
        assert!(check(&info).is_ok());
    }

    #[test]
    fn check_prompt_approved() {
        let info = CrossingInfo::new("test.rs", 1, "declassify", "");
        info.set_policy(Policy::Prompt);
        inject_response(true);
        assert!(check(&info).is_ok());
    }

    #[test]
    fn check_prompt_denied() {
        let info = CrossingInfo::new("test.rs", 1, "declassify", "");
        info.set_policy(Policy::Prompt);
        inject_response(false);
        assert!(check(&info).is_err());
    }

    #[test]
    fn check_no_policy_fails() {
        let info = CrossingInfo::new("test.rs", 1, "declassify", "");
        assert!(check(&info).is_err());
    }
}
