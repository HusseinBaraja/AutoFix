use std::path::{Path, PathBuf};

const ALLOWED_PROCESS_NAMES: [&str; 1] = ["Autofix.exe"];

pub(crate) struct SiblingDisappearanceMonitor {
    observed_sibling_ids: Vec<u32>,
}

impl SiblingDisappearanceMonitor {
    pub(crate) fn new() -> Self {
        Self {
            observed_sibling_ids: Vec::new(),
        }
    }

    pub(crate) fn shutdown_requested(&mut self) -> bool {
        match native::current_process_context() {
            Ok(context) => self.shutdown_requested_with(&native::RealProcessController, &context),
            Err(error) => {
                tracing::debug!("AutoFix sibling monitor skipped: {}", error);
                false
            }
        }
    }

    fn shutdown_requested_with(
        &mut self,
        controller: &impl ProcessController,
        context: &CurrentProcessContext,
    ) -> bool {
        let sibling_ids = controller
            .list_processes()
            .into_iter()
            .filter(|process| is_sibling_autofix_process(process, context))
            .map(|process| process.process_id)
            .collect::<Vec<_>>();

        if self
            .observed_sibling_ids
            .iter()
            .any(|process_id| !sibling_ids.contains(process_id))
        {
            tracing::info!("previously observed AutoFix sibling disappeared; shutdown requested");
            return true;
        }

        for process_id in sibling_ids {
            if !self.observed_sibling_ids.contains(&process_id) {
                self.observed_sibling_ids.push(process_id);
            }
        }

        false
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

trait ProcessController {
    fn list_processes(&self) -> Vec<ProcessSnapshot>;
}

fn is_sibling_autofix_process(process: &ProcessSnapshot, context: &CurrentProcessContext) -> bool {
    process.process_id != context.process_id
        && process.owner_sid == context.owner_sid
        && ALLOWED_PROCESS_NAMES
            .iter()
            .any(|name| process.executable_name.eq_ignore_ascii_case(name))
        && is_path_rooted_in(&process.executable_path, &context.trusted_root)
}

fn trusted_root_for_executable(executable_path: &Path, manifest_dir: &Path) -> Option<PathBuf> {
    let workspace_root = manifest_dir.parent().unwrap_or(manifest_dir);
    if is_path_rooted_in(executable_path, workspace_root) {
        return Some(workspace_root.to_path_buf());
    }

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
        mem::{size_of, zeroed},
        os::windows::ffi::OsStringExt,
        path::{Path, PathBuf},
        ptr::null_mut,
    };
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, GetLastError, LocalFree, ERROR_INSUFFICIENT_BUFFER, HANDLE,
            INVALID_HANDLE_VALUE, MAX_PATH,
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
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
    };

    pub(super) struct RealProcessController;

    impl ProcessController for RealProcessController {
        fn list_processes(&self) -> Vec<ProcessSnapshot> {
            list_processes()
        }
    }

    pub(super) fn current_process_context() -> Result<CurrentProcessContext, String> {
        let process_id = unsafe { GetCurrentProcessId() };
        let executable_path = std::env::current_exe().map_err(|error| error.to_string())?;
        let trusted_root =
            trusted_root_for_executable(&executable_path, Path::new(env!("CARGO_MANIFEST_DIR")))
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
        let length = loop {
            let mut length = buffer.len() as u32;
            let ok =
                unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) };
            if ok != 0 || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
                break if ok == 0 { 0 } else { length };
            }

            buffer.resize(buffer.len() * 2, 0);
        };
        unsafe {
            CloseHandle(process);
        }

        if length == 0 {
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
}

#[cfg(not(windows))]
mod native {
    use super::{CurrentProcessContext, ProcessController, ProcessSnapshot};

    pub(super) struct RealProcessController;

    impl ProcessController for RealProcessController {
        fn list_processes(&self) -> Vec<ProcessSnapshot> {
            Vec::new()
        }
    }

    pub(super) fn current_process_context() -> Result<CurrentProcessContext, String> {
        Err("AutoFix process-group shutdown is only supported on Windows".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_same_user_allowlisted_process_under_trusted_root() {
        let context = context();
        let process = process(42, "S-1-5-21", "Autofix.exe", r"C:\AutoFix\Autofix.exe");

        assert!(is_sibling_autofix_process(&process, &context));
    }

    #[test]
    fn rejects_different_owner_sid() {
        let context = context();
        let process = process(42, "S-1-5-99", "Autofix.exe", r"C:\AutoFix\Autofix.exe");

        assert!(!is_sibling_autofix_process(&process, &context));
    }

    #[test]
    fn rejects_same_name_outside_trusted_root() {
        let context = context();
        let process = process(42, "S-1-5-21", "Autofix.exe", r"C:\Other\Autofix.exe");

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
        let process = process(7, "S-1-5-21", "Autofix.exe", r"C:\AutoFix\Autofix.exe");

        assert!(!is_sibling_autofix_process(&process, &context));
    }

    #[test]
    fn dev_build_uses_workspace_root_for_settings_ui_matching() {
        let manifest_dir = PathBuf::from(r"C:\Repo\AutoFix\app-core-rust");
        let engine_host =
            PathBuf::from(r"C:\Repo\AutoFix\ui\settings-ui\bin\Debug\net8.0-windows\Autofix.exe");
        let trusted_root = trusted_root_for_executable(&engine_host, &manifest_dir).unwrap();
        let context = CurrentProcessContext {
            process_id: 7,
            owner_sid: "S-1-5-21".to_owned(),
            trusted_root,
        };
        let settings = process(
            42,
            "S-1-5-21",
            "Autofix.exe",
            r"C:\Repo\AutoFix\ui\settings-ui\bin\Debug\net8.0-windows\Autofix.exe",
        );

        assert!(is_sibling_autofix_process(&settings, &context));
    }

    #[test]
    fn installed_build_uses_executable_directory_as_trusted_root() {
        let manifest_dir = PathBuf::from(r"C:\Repo\AutoFix\app-core-rust");
        let engine_host = PathBuf::from(r"C:\Program Files\AutoFix\Autofix.exe");
        let trusted_root = trusted_root_for_executable(&engine_host, &manifest_dir).unwrap();

        assert_eq!(trusted_root, PathBuf::from(r"C:\Program Files\AutoFix"));
    }

    #[test]
    fn sibling_disappearance_monitor_triggers_only_after_seen() {
        let mut monitor = SiblingDisappearanceMonitor::new();
        let first = FakeProcessController::new(Vec::new());
        assert!(!monitor.shutdown_requested_with(&first, &context()));

        let seen = FakeProcessController::new(vec![process(
            42,
            "S-1-5-21",
            "Autofix.exe",
            r"C:\AutoFix\Autofix.exe",
        )]);
        assert!(!monitor.shutdown_requested_with(&seen, &context()));

        let gone = FakeProcessController::new(Vec::new());
        assert!(monitor.shutdown_requested_with(&gone, &context()));
    }

    #[test]
    fn sibling_disappearance_monitor_triggers_when_one_of_multiple_siblings_disappears() {
        let mut monitor = SiblingDisappearanceMonitor::new();
        let both = FakeProcessController::new(vec![
            process(42, "S-1-5-21", "Autofix.exe", r"C:\AutoFix\Autofix.exe"),
            process(43, "S-1-5-21", "Autofix.exe", r"C:\AutoFix\Autofix.exe"),
        ]);
        assert!(!monitor.shutdown_requested_with(&both, &context()));

        let remaining = FakeProcessController::new(vec![process(
            43,
            "S-1-5-21",
            "Autofix.exe",
            r"C:\AutoFix\Autofix.exe",
        )]);
        assert!(monitor.shutdown_requested_with(&remaining, &context()));
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
    }

    impl FakeProcessController {
        fn new(processes: Vec<ProcessSnapshot>) -> Self {
            Self { processes }
        }
    }

    impl ProcessController for FakeProcessController {
        fn list_processes(&self) -> Vec<ProcessSnapshot> {
            self.processes.clone()
        }
    }
}
