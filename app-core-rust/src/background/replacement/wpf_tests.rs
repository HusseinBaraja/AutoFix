//! Opt-in real WPF provider tests. Commands touch only the owned disposable fixture.
use super::*;
use crate::background::{
    input_listener,
    target::{detect_focused_target, TargetDetection},
};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    os::windows::process::CommandExt,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

struct Fixture {
    child: Child,
    stdin: ChildStdin,
    replies: mpsc::Receiver<Value>,
    previous: isize,
    window: isize,
}
impl Fixture {
    fn start() -> Self {
        let path = std::env::var("AUTOFIX_WPF_FIXTURE")
            .expect("build the WPF fixture and set AUTOFIX_WPF_FIXTURE to its executable");
        let previous =
            unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() } as isize;
        let mut child = Command::new(path)
            .arg("--automation")
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, replies) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let Ok(reply) = serde_json::from_str(&line) else {
                    continue;
                };
                if sender.send(reply).is_err() {
                    break;
                }
            }
        });
        let mut fixture = Self {
            child,
            stdin,
            replies,
            previous,
            window: 0,
        };
        fixture.window = fixture.reply()["ready"]
            .as_i64()
            .expect("fixture ready handle") as isize;
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(
                fixture.child.id(),
            );
        }
        fixture
    }
    fn reply(&self) -> Value {
        self.replies
            .recv_timeout(Duration::from_secs(5))
            .expect("fixture response timeout")
    }
    fn call(&mut self, command: Value) -> Value {
        writeln!(self.stdin, "{command}").unwrap();
        self.stdin.flush().unwrap();
        self.reply()
    }
    fn setup(&mut self, text: &str, read_only: bool) -> FocusedTarget {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(self.window as _);
        }
        self.call(json!({"command":"setup", "text":text, "start":text.encode_utf16().count(), "length":0, "readOnly":read_only}));
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            if let TargetDetection::Available(target) = detect_focused_target() {
                if target.process_id == self.child.id()
                    && target.correction_eligibility() == CorrectionEligibility::Allowed
                {
                    return target;
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "WPF fixture could not acquire authorized focus"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "{}", json!({"command":"close"}));
        let _ = self.stdin.flush();
        for _ in 0..40 {
            if self.child.try_wait().ok().flatten().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        if self.previous != 0 {
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(
                    self.previous as _,
                );
            }
        }
    }
}

fn stamp() -> InputStamp {
    InputStamp {
        position: input_listener::current_position_generation(),
        sequence: input_listener::current_input_sequence(),
    }
}

#[test]
#[ignore = "requires an interactive desktop and AUTOFIX_WPF_FIXTURE; owns its fixture"]
fn wpf_uia_replacement_and_app_undo() {
    let mut fixture = Fixture::start();
    for replacement in ["the", "é😃", "العربية", ""] {
        let target = fixture.setup("old teh", false);
        let plan = ReplacementPlan {
            target: &target,
            original: "teh",
            replacement,
            following: "",
            selected_text: false,
            stamp: stamp(),
        };
        let result = ReplacementEngine::execute(&plan, false);
        assert!(result.success, "{result:?}");
        assert_eq!(result.method, Some(ReplacementMethod::UiAutomation));
        let observed = fixture.call(json!({"command":"snapshot"}));
        assert_eq!(observed["text"], format!("old {replacement}"));
        assert_eq!(observed["start"], 4 + replacement.encode_utf16().count());
        assert_eq!(observed["length"], 0);
        let TargetDetection::Available(after) = detect_focused_target() else {
            panic!("fixture focus lost")
        };
        let undo = ReplacementPlan {
            target: &after,
            original: replacement,
            replacement: "teh",
            following: "",
            selected_text: false,
            stamp: stamp(),
        };
        let restored = ReplacementEngine::execute(&undo, false);
        assert!(restored.success, "{restored:?}");
        let observed = fixture.call(json!({"command":"snapshot"}));
        assert_eq!(observed["text"], "old teh");
        assert_eq!(observed["start"], 7);
    }
    // Equal-width selected-character patches preserve the unowned after-caret
    // suffix even when multi-character keyboard insertion cannot be authorized.
    let target = fixture.setup("old teh AFTER", false);
    fixture.call(json!({"command":"setup", "text":"old teh AFTER", "start":7, "length":0}));
    let plan = ReplacementPlan {
        target: &target,
        original: "teh",
        replacement: "the",
        following: "",
        selected_text: false,
        stamp: stamp(),
    };
    let corrected = ReplacementEngine::execute(&plan, false);
    assert!(corrected.success, "{corrected:?}");
    let observed = fixture.call(json!({"command":"snapshot"}));
    assert_eq!(observed["text"], "old the AFTER");
    assert_eq!(observed["start"], 7);
    assert_eq!(observed["length"], 0);
    let undo = ReplacementPlan {
        original: "the",
        replacement: "teh",
        ..plan
    };
    let restored = ReplacementEngine::execute(&undo, false);
    assert!(restored.success, "{restored:?}");
    assert_eq!(
        fixture.call(json!({"command":"snapshot"}))["text"],
        "old teh AFTER"
    );

    let target = fixture.setup("old teh", true);
    let plan = ReplacementPlan {
        target: &target,
        original: "teh",
        replacement: "the",
        following: "",
        selected_text: false,
        stamp: stamp(),
    };
    let refused = ReplacementEngine::execute(&plan, false);
    assert!(!refused.success && !refused.may_have_changed, "{refused:?}");
    assert_eq!(
        fixture.call(json!({"command":"snapshot"}))["text"],
        "old teh"
    );
}
