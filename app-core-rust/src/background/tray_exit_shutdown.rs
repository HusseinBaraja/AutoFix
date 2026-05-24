use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const ALLOWED_PROCESS_NAMES: [&str; 2] = ["background-engine.exe", "AutoFix.SettingsUi.exe"];
const GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);

pub(crate) fn shutdown_sibling_autofix_processes_for_tray_exit() {
    match native::current_process_context() {
        Ok(context) => {
            let outcome = shutdown_sibling_processes(&native::RealProcessController, &context);
            tracing::info!(
                matched = outcome.matched,
                graceful_succeeded = outcome.graceful_succeeded,
                graceful_timed_out = outcome.graceful_timed_out,
                force_kill_attempted = outcome.force_kill_attempted,
                force_kill_succeeded = outcome.force_kill_succeeded,
                force_kill_failed = outcome.force_kill_failed,
                "overall tray-exit shutdown completed"
            );
        }
        Err(error) => tracing::warn!("tray-exit sibling shutdown skipped: {}", error),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessSnapshot {
    process_id: u32,
    owner_sid: String,
    executable_name: String,
    executable_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CurrentProcessContext {
    process_id: u32,
    owner_sid: String,
    trusted_root: PathBuf,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ShutdownOutcome {
    matched: usize,
    graceful_succeeded: usize,
    graceful_timed_out: usize,
    force_kill_attempted: usize,
    force_kill_succeeded: usize,
    force_kill_failed: usize,
}

trait ProcessController {
    fn list_processes(&self) -> Vec<ProcessSnapshot>;
    fn request_graceful_exit(&self, process: &ProcessSnapshot) -> bool;
    fn wait_for_exit(&self, process: &ProcessSnapshot, timeout: Duration) -> bool;
    fn force_kill(&self, process: &ProcessSnapshot) -> bool;
}

fn shutdown_sibling_processes(
    controller: &impl ProcessController,
    context: &CurrentProcessContext,
) -> ShutdownOutcome {
    let targets = controller
        .list_processes()
        .into_iter()
        .filter(|process| is_sibling_autofix_process(process, context))
        .collect::<Vec<_>>();

    let mut outcome = ShutdownOutcome {
        matched: targets.len(),
        ..ShutdownOutcome::default()
    };

    for target in targets {
        tracing::info!(
            process_id = target.process_id,
            executable_name = %target.executable_name,
            executable_path = %target.executable_path.display(),
            "matched AutoFix process discovered for tray-exit shutdown"
        );

        let graceful_requested = controller.request_graceful_exit(&target);
        if graceful_requested && controller.wait_for_exit(&target, GRACEFUL_SHUTDOWN_TIMEOUT) {
            outcome.graceful_succeeded += 1;
            tracing::info!(
                process_id = target.process_id,
                executable_name = %target.executable_name,
                "graceful exit succeeded for AutoFix process"
            );
            continue;
        }

        outcome.graceful_timed_out += 1;
        tracing::warn!(
            process_id = target.process_id,
            executable_name = %target.executable_name,
            "graceful exit timed out for AutoFix process"
        );

        outcome.force_kill_attempted += 1;
        if controller.force_kill(&target) {
            outcome.force_kill_succeeded += 1;
            tracing::warn!(
                process_id = target.process_id,
                executable_name = %target.executable_name,
                "force-kill succeeded for AutoFix process"
            );
        } else {
            outcome.force_kill_failed += 1;
            tracing::error!(
                process_id = target.process_id,
                executable_name = %target.executable_name,
                "force-kill failed for AutoFix process"
            );
        }
    }

    outcome
}

fn is_sibling_autofix_process(process: &ProcessSnapshot, context: &CurrentProcessContext) -> bool {
    process.process_id != context.process_id
        && process.owner_sid == context.owner_sid
        && ALLOWED_PROCESS_NAMES
            .iter()
            .any(|name| process.executable_name.eq_ignore_ascii_case(name))
        && is_path_rooted_in(&process.executable_path, &context.trusted_root)
}

fn trusted_root_for_executable(executable_path: &Path) -> Option<PathBuf> {
    executable_path.parent().map(Path::to_path_buf)
}

fn is_path_rooted_in(path: &Path, root: &Path) -> bool {
    let path = normalize_path_text(path);
    let root = normalize_path_text(root);

    path == root
        || path
            .strip_prefix(&(root + "\\"))
            .is_some_and(|suffix| !suffix.is_empty())
}

fn normalize_path_text(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").to_lowercase()
}

#[cfg(windows)]
mod native {
    use super::{
        trusted_root_for_executable, CurrentProcessContext, ProcessController, ProcessSnapshot,
    };
    use std::{
        ffi::OsString,
        iter::once,
        mem::{size_of, zeroed},
        os::windows::ffi::{OsStrExt, OsStringExt},
        path::{Path, PathBuf},
        ptr::null_mut,
        time::Duration,
    };
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, LocalFree, BOOL, HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM, MAX_PATH,
            TRUE, WAIT_OBJECT_0,
        },
        Security::{
            Authorization::ConvertSidToStringSidW, GetTokenInformation, TokenUser, TOKEN_QUERY,
            TOKEN_USER,
        },
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                GetCurrentProcessId, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
                TerminateProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
                PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
            },
        },
        UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_CLOSE},
    };

    pub(super) struct RealProcessController;

    impl ProcessController for RealProcessController {
        fn list_processes(&self) -> Vec<ProcessSnapshot> {
            list_processes()
        }

        fn request_graceful_exit(&self, process: &ProcessSnapshot) -> bool {
            request_window_close(process.process_id)
        }

        fn wait_for_exit(&self, process: &ProcessSnapshot, timeout: Duration) -> bool {
            wait_for_exit(process.process_id, timeout)
        }

        fn force_kill(&self, process: &ProcessSnapshot) -> bool {
            force_kill(process.process_id)
        }
    }

    pub(super) fn current_process_context() -> Result<CurrentProcessContext, String> {
        let process_id = unsafe { GetCurrentProcessId() };
        let executable_path = std::env::current_exe().map_err(|error| error.to_string())?;
        let trusted_root = trusted_root_for_executable(&executable_path)
            .ok_or_else(|| "current executable has no parent directory".to_owned())?;
        let owner_sid = owner_sid_for_process(process_id)?;

        Ok(CurrentProcessContext {
            process_id,
            owner_sid,
            trusted_root,
        })
    }

    fn list_processes() -> Vec<ProcessSnapshot> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            tracing::warn!(
                "failed to create process snapshot: {}",
                std::io::Error::last_os_error()
            );
            return Vec::new();
        }

        let mut entry = unsafe { zeroed::<PROCESSENTRY32W>() };
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;

        let mut processes = Vec::new();
        let mut has_entry = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        while has_entry {
            let process_id = entry.th32ProcessID;
            if let (Some(executable_path), Ok(owner_sid)) =
                (process_path(process_id), owner_sid_for_process(process_id))
            {
                let executable_name = executable_path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| process_name_from_entry(&entry));

                processes.push(ProcessSnapshot {
                    process_id,
                    owner_sid,
                    executable_name,
                    executable_path,
                });
            }

            has_entry = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }

        unsafe {
            CloseHandle(snapshot);
        }
        processes
    }

    fn process_path(process_id: u32) -> Option<PathBuf> {
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        if process.is_null() {
            return None;
        }

        let mut buffer = vec![0_u16; MAX_PATH as usize];
        let mut length = buffer.len() as u32;
        let ok =
            unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) };
        unsafe {
            CloseHandle(process);
        }

        if ok == 0 || length == 0 {
            None
        } else {
            Some(PathBuf::from(OsString::from_wide(
                &buffer[..length as usize],
            )))
        }
    }

    fn owner_sid_for_process(process_id: u32) -> Result<String, String> {
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        if process.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }

        let mut token: HANDLE = null_mut();
        let token_opened = unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) };
        unsafe {
            CloseHandle(process);
        }
        if token_opened == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }

        let mut needed = 0;
        unsafe {
            GetTokenInformation(token, TokenUser, null_mut(), 0, &mut needed);
        }
        if needed == 0 {
            unsafe {
                CloseHandle(token);
            }
            return Err(std::io::Error::last_os_error().to_string());
        }

        let mut buffer = vec![0_u8; needed as usize];
        let ok = unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr() as *mut _,
                needed,
                &mut needed,
            )
        };
        unsafe {
            CloseHandle(token);
        }
        if ok == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }

        let token_user = unsafe { &*(buffer.as_ptr() as *const TOKEN_USER) };
        sid_to_string(token_user.User.Sid)
    }

    fn sid_to_string(sid: *mut std::ffi::c_void) -> Result<String, String> {
        let mut sid_text: *mut u16 = null_mut();
        let ok = unsafe { ConvertSidToStringSidW(sid, &mut sid_text) };
        if ok == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }

        let mut length = 0;
        while unsafe { *sid_text.add(length) } != 0 {
            length += 1;
        }
        let sid = OsString::from_wide(unsafe { std::slice::from_raw_parts(sid_text, length) })
            .to_string_lossy()
            .into_owned();
        unsafe {
            LocalFree(sid_text as *mut _);
        }
        Ok(sid)
    }

    struct CloseWindowContext {
        process_id: u32,
        posted: bool,
    }

    fn request_window_close(process_id: u32) -> bool {
        let mut context = CloseWindowContext {
            process_id,
            posted: false,
        };

        unsafe {
            EnumWindows(
                Some(enum_window_for_process),
                &mut context as *mut CloseWindowContext as LPARAM,
            );
        }

        context.posted
    }

    unsafe extern "system" fn enum_window_for_process(window: HWND, parameter: LPARAM) -> BOOL {
        let context = &mut *(parameter as *mut CloseWindowContext);
        let mut window_process_id = 0;
        GetWindowThreadProcessId(window, &mut window_process_id);
        if window_process_id == context.process_id {
            if PostMessageW(window, WM_CLOSE, 0, 0) != 0 {
                context.posted = true;
            }
        }

        TRUE
    }

    fn wait_for_exit(process_id: u32, timeout: Duration) -> bool {
        let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, process_id) };
        if process.is_null() {
            return true;
        }

        let result = unsafe { WaitForSingleObject(process, timeout.as_millis() as u32) };
        unsafe {
            CloseHandle(process);
        }
        result == WAIT_OBJECT_0
    }

    fn force_kill(process_id: u32) -> bool {
        let process = unsafe { OpenProcess(PROCESS_TERMINATE, 0, process_id) };
        if process.is_null() {
            return false;
        }

        let ok = unsafe { TerminateProcess(process, 1) } != 0;
        unsafe {
            CloseHandle(process);
        }
        ok
    }

    fn process_name_from_entry(entry: &PROCESSENTRY32W) -> String {
        let nul = entry
            .szExeFile
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(entry.szExeFile.len());
        OsString::from_wide(&entry.szExeFile[..nul])
            .to_string_lossy()
            .into_owned()
    }

    #[allow(dead_code)]
    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(once(0)).collect()
    }
}

#[cfg(not(windows))]
mod native {
    use super::{CurrentProcessContext, ProcessController, ProcessSnapshot};
    use std::time::Duration;

    pub(super) struct RealProcessController;

    impl ProcessController for RealProcessController {
        fn list_processes(&self) -> Vec<ProcessSnapshot> {
            Vec::new()
        }

        fn request_graceful_exit(&self, _process: &ProcessSnapshot) -> bool {
            false
        }

        fn wait_for_exit(&self, _process: &ProcessSnapshot, _timeout: Duration) -> bool {
            true
        }

        fn force_kill(&self, _process: &ProcessSnapshot) -> bool {
            false
        }
    }

    pub(super) fn current_process_context() -> Result<CurrentProcessContext, String> {
        Err("tray-exit sibling shutdown is only supported on Windows".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn accepts_same_user_allowlisted_process_under_trusted_root() {
        let context = context();
        let process = process(
            42,
            "S-1-5-21",
            "background-engine.exe",
            r"C:\AutoFix\background-engine.exe",
        );

        assert!(is_sibling_autofix_process(&process, &context));
    }

    #[test]
    fn rejects_different_owner_sid() {
        let context = context();
        let process = process(
            42,
            "S-1-5-99",
            "background-engine.exe",
            r"C:\AutoFix\background-engine.exe",
        );

        assert!(!is_sibling_autofix_process(&process, &context));
    }

    #[test]
    fn rejects_same_name_outside_trusted_root() {
        let context = context();
        let process = process(
            42,
            "S-1-5-21",
            "background-engine.exe",
            r"C:\Other\background-engine.exe",
        );

        assert!(!is_sibling_autofix_process(&process, &context));
    }

    #[test]
    fn rejects_non_allowlisted_executable() {
        let context = context();
        let process = process(42, "S-1-5-21", "notepad.exe", r"C:\AutoFix\notepad.exe");

        assert!(!is_sibling_autofix_process(&process, &context));
    }

    #[test]
    fn excludes_current_process_from_sibling_pass() {
        let context = context();
        let process = process(
            7,
            "S-1-5-21",
            "background-engine.exe",
            r"C:\AutoFix\background-engine.exe",
        );

        assert!(!is_sibling_autofix_process(&process, &context));
    }

    #[test]
    fn graceful_success_does_not_force_kill() {
        let controller = FakeProcessController::new(vec![process(
            42,
            "S-1-5-21",
            "AutoFix.SettingsUi.exe",
            r"C:\AutoFix\AutoFix.SettingsUi.exe",
        )])
        .with_graceful_exit(42);

        let outcome = shutdown_sibling_processes(&controller, &context());

        assert_eq!(outcome.graceful_succeeded, 1);
        assert_eq!(outcome.force_kill_attempted, 0);
        assert_eq!(controller.actions(), ["graceful:42", "wait:42"]);
    }

    #[test]
    fn graceful_timeout_triggers_force_kill() {
        let controller = FakeProcessController::new(vec![process(
            42,
            "S-1-5-21",
            "AutoFix.SettingsUi.exe",
            r"C:\AutoFix\AutoFix.SettingsUi.exe",
        )])
        .with_force_kill_success(42);

        let outcome = shutdown_sibling_processes(&controller, &context());

        assert_eq!(outcome.graceful_timed_out, 1);
        assert_eq!(outcome.force_kill_attempted, 1);
        assert_eq!(outcome.force_kill_succeeded, 1);
        assert_eq!(controller.actions(), ["graceful:42", "kill:42"]);
    }

    #[test]
    fn failure_on_one_target_does_not_block_remaining_targets() {
        let controller = FakeProcessController::new(vec![
            process(
                42,
                "S-1-5-21",
                "AutoFix.SettingsUi.exe",
                r"C:\AutoFix\AutoFix.SettingsUi.exe",
            ),
            process(
                43,
                "S-1-5-21",
                "background-engine.exe",
                r"C:\AutoFix\background-engine.exe",
            ),
        ])
        .with_force_kill_success(43);

        let outcome = shutdown_sibling_processes(&controller, &context());

        assert_eq!(outcome.force_kill_attempted, 2);
        assert_eq!(outcome.force_kill_succeeded, 1);
        assert_eq!(outcome.force_kill_failed, 1);
        assert_eq!(
            controller.actions(),
            ["graceful:42", "kill:42", "graceful:43", "kill:43"]
        );
    }

    #[test]
    fn self_shutdown_is_ordered_last_by_excluding_current_process() {
        let controller = FakeProcessController::new(vec![
            process(
                7,
                "S-1-5-21",
                "background-engine.exe",
                r"C:\AutoFix\background-engine.exe",
            ),
            process(
                42,
                "S-1-5-21",
                "AutoFix.SettingsUi.exe",
                r"C:\AutoFix\AutoFix.SettingsUi.exe",
            ),
        ])
        .with_graceful_exit(42);

        let outcome = shutdown_sibling_processes(&controller, &context());

        assert_eq!(outcome.matched, 1);
        assert_eq!(controller.actions(), ["graceful:42", "wait:42"]);
    }

    fn context() -> CurrentProcessContext {
        CurrentProcessContext {
            process_id: 7,
            owner_sid: "S-1-5-21".to_owned(),
            trusted_root: PathBuf::from(r"C:\AutoFix"),
        }
    }

    fn process(
        process_id: u32,
        owner_sid: &str,
        executable_name: &str,
        executable_path: &str,
    ) -> ProcessSnapshot {
        ProcessSnapshot {
            process_id,
            owner_sid: owner_sid.to_owned(),
            executable_name: executable_name.to_owned(),
            executable_path: PathBuf::from(executable_path),
        }
    }

    struct FakeProcessController {
        processes: Vec<ProcessSnapshot>,
        graceful_exits: Vec<u32>,
        force_kill_successes: Vec<u32>,
        actions: RefCell<Vec<String>>,
    }

    impl FakeProcessController {
        fn new(processes: Vec<ProcessSnapshot>) -> Self {
            Self {
                processes,
                graceful_exits: Vec::new(),
                force_kill_successes: Vec::new(),
                actions: RefCell::new(Vec::new()),
            }
        }

        fn with_graceful_exit(mut self, process_id: u32) -> Self {
            self.graceful_exits.push(process_id);
            self
        }

        fn with_force_kill_success(mut self, process_id: u32) -> Self {
            self.force_kill_successes.push(process_id);
            self
        }

        fn actions(&self) -> Vec<String> {
            self.actions.borrow().clone()
        }
    }

    impl ProcessController for FakeProcessController {
        fn list_processes(&self) -> Vec<ProcessSnapshot> {
            self.processes.clone()
        }

        fn request_graceful_exit(&self, process: &ProcessSnapshot) -> bool {
            self.actions
                .borrow_mut()
                .push(format!("graceful:{}", process.process_id));
            self.graceful_exits.contains(&process.process_id)
        }

        fn wait_for_exit(&self, process: &ProcessSnapshot, _timeout: Duration) -> bool {
            self.actions
                .borrow_mut()
                .push(format!("wait:{}", process.process_id));
            self.graceful_exits.contains(&process.process_id)
        }

        fn force_kill(&self, process: &ProcessSnapshot) -> bool {
            self.actions
                .borrow_mut()
                .push(format!("kill:{}", process.process_id));
            self.force_kill_successes.contains(&process.process_id)
        }
    }
}
