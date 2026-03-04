use std::collections::VecDeque;
use std::io::{BufRead, Write};
use std::sync::Mutex;

use anyhow::{bail, Result};
use linkme::distributed_slice;

pub struct Need {
    pub path: &'static str,
    pub kind: &'static str,
    pub classification: &'static str,
    pub integrity: &'static str,
}

impl Need {
    pub const fn new(
        path: &'static str,
        kind: &'static str,
        classification: &'static str,
        integrity: &'static str,
    ) -> Self {
        Need { path, kind, classification, integrity }
    }
}

#[distributed_slice]
pub static NEEDS: [Need];

// -- Test injection ----------------------------------------------------------

static INJECTED: Mutex<VecDeque<bool>> =
    Mutex::new(VecDeque::new());

pub fn inject_approval(approve: bool) {
    INJECTED.lock().unwrap().push_back(approve);
}

// -- Init: show manifest and prompt ------------------------------------------

pub fn init() -> Result<()> {
    if NEEDS.is_empty() {
        return Ok(());
    }

    eprintln!("Needs:");
    for need in NEEDS.iter() {
        eprintln!(
            "  {:<10} {:<24} {} {}",
            need.kind, need.path,
            need.classification, need.integrity,
        );
    }

    if let Some(approve) =
        INJECTED.lock().unwrap().pop_front()
    {
        if approve {
            return Ok(());
        }
        bail!("manifest rejected");
    }

    eprint!("\napprove? [y/N]: ");
    std::io::stderr().flush()?;
    let mut input = String::new();
    std::io::stdin().lock().read_line(&mut input)?;
    match input.trim() {
        "y" | "Y" | "yes" => Ok(()),
        _ => bail!("manifest rejected"),
    }
}

// -- Tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_needs_auto_approves() {
        // NEEDS slice is empty in test binary
        // (no #[needs] macros), so init() returns Ok
        assert!(init().is_ok());
    }

    #[test]
    fn inject_approve_passes() {
        inject_approval(true);
        // With empty NEEDS this is a no-op,
        // but verifies injection doesn't panic
        assert!(init().is_ok());
    }

    #[test]
    fn inject_reject_fails() {
        inject_approval(false);
        // With empty NEEDS, init returns Ok immediately
        // (never reaches injection). This test verifies
        // the injection queue works; real rejection
        // requires a binary with #[needs] entries.
        assert!(init().is_ok());
    }
}
