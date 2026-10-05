//! Bind hosted text providers to the foreground application's live process tree.
use std::collections::{HashMap, HashSet};
use windows_sys::Win32::{
    Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE},
    Security::{
        GetLengthSid, GetTokenInformation, IsValidSid, TokenElevation, TokenSessionId, TokenUser,
        TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER,
    },
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        Threading::{
            GetProcessTimes, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
};

const MAX_ANCESTORS: usize = 16;

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[derive(Clone)]
struct ProcessIdentity {
    id: u32,
    parent: u32,
    created: u64,
    session: u32,
    owner: Vec<u8>,
    elevated: bool,
}

fn verified_descendant(
    root: &ProcessIdentity,
    provider: u32,
    mut read: impl FnMut(u32) -> Option<ProcessIdentity>,
) -> bool {
    if root.id == 0 || provider == 0 || root.elevated || root.owner.is_empty() {
        return false;
    }
    let mut current = provider;
    let mut child_created = u64::MAX;
    let mut visited = HashSet::new();
    for _ in 0..MAX_ANCESTORS {
        if !visited.insert(current) {
            return false;
        }
        let identity = if current == root.id {
            root.clone()
        } else {
            let Some(identity) = read(current) else {
                return false;
            };
            identity
        };
        if identity.id != current
            || identity.elevated
            || identity.owner != root.owner
            || identity.session != root.session
            || identity.created == 0
            || identity.created > child_created
        {
            return false;
        }
        if current == root.id {
            return true;
        }
        child_created = identity.created;
        current = identity.parent;
        if current == 0 {
            return false;
        }
    }
    false
}

/// Same-process providers retain the existing path. Cross-process providers require
/// live ancestry, matching token user/session, no elevation, and creation-time order.
/// Call only for a focused provider already found inside a keyboard-owned UIA host.
pub(in crate::background) fn matches(foreground: u32, provider: u32) -> bool {
    if foreground == 0 || provider == 0 {
        return false;
    }
    if foreground == provider {
        return true;
    }
    let Some((root, _root_handle)) = read_identity(foreground, 0) else {
        return false;
    };
    let Some(parents) = process_parents() else {
        return false;
    };
    // Keep each queried process object alive throughout the proof to prevent PID reuse.
    let mut held_handles = Vec::new();
    verified_descendant(&root, provider, |id| {
        let parent = *parents.get(&id)?;
        let (identity, handle) = read_identity(id, parent)?;
        held_handles.push(handle);
        Some(identity)
    })
}

fn process_parents() -> Option<HashMap<u32, u32>> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
            return None;
        }
        let snapshot = Handle(snapshot);
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snapshot.0, &mut entry) == 0 {
            return None;
        }
        let mut parents = HashMap::new();
        loop {
            parents.insert(entry.th32ProcessID, entry.th32ParentProcessID);
            if Process32NextW(snapshot.0, &mut entry) == 0 {
                break;
            }
        }
        Some(parents)
    }
}

fn read_identity(id: u32, parent: u32) -> Option<(ProcessIdentity, Handle)> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, id);
        if process.is_null() {
            return None;
        }
        let process = Handle(process);
        let mut created: FILETIME = std::mem::zeroed();
        let mut exited: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        if GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user) == 0
            || exited.dwLowDateTime != 0
            || exited.dwHighDateTime != 0
        {
            return None;
        }
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(process.0, TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let token = Handle(token);
        let mut needed = 0;
        let mut session = 0_u32;
        if GetTokenInformation(
            token.0,
            TokenSessionId,
            &mut session as *mut _ as *mut _,
            std::mem::size_of::<u32>() as u32,
            &mut needed,
        ) == 0
        {
            return None;
        }
        let mut elevation: TOKEN_ELEVATION = std::mem::zeroed();
        if GetTokenInformation(
            token.0,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut needed,
        ) == 0
        {
            return None;
        }
        GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut needed);
        if needed == 0 || needed > 4096 {
            return None;
        }
        let size = needed as usize;
        let mut buffer = vec![0_usize; size.div_ceil(std::mem::size_of::<usize>())];
        if GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr() as *mut _,
            size as u32,
            &mut needed,
        ) == 0
        {
            return None;
        }
        let token_user = &*(buffer.as_ptr() as *const TOKEN_USER);
        let sid = token_user.User.Sid;
        if IsValidSid(sid) == 0 {
            return None;
        }
        let sid_len = GetLengthSid(sid) as usize;
        if sid_len == 0 || sid_len > 1024 {
            return None;
        }
        let owner = std::slice::from_raw_parts(sid as *const u8, sid_len).to_vec();
        Some((
            ProcessIdentity {
                id,
                parent,
                created: ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64,
                session,
                owner,
                elevated: elevation.TokenIsElevated != 0,
            },
            process,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_child_ownership_proof_matches_the_live_process_tree() {
        use std::os::windows::process::CommandExt;
        use std::process::{Child, Command, Stdio};
        struct OwnedChild(Child);
        impl Drop for OwnedChild {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let child = OwnedChild(
            Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "Start-Sleep -Seconds 30",
                ])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let root = std::process::id();
        assert!(matches(root, child.0.id()));
        assert!(!matches(child.0.id(), root));
        assert!(!matches(root, 0));
    }
    fn identity(id: u32, parent: u32, created: u64) -> ProcessIdentity {
        ProcessIdentity {
            id,
            parent,
            created,
            session: 1,
            owner: vec![1, 2, 3],
            elevated: false,
        }
    }

    #[test]
    fn hosted_provider_requires_a_live_same_user_session_process_chain() {
        let root = identity(1, 0, 10);
        let processes = [identity(2, 1, 20), identity(3, 2, 30)];
        assert!(verified_descendant(&root, 3, |id| processes
            .iter()
            .find(|p| p.id == id)
            .cloned()));
        for changed in 0..7 {
            let mut child = identity(2, 1, 20);
            match changed {
                0 => child.owner = vec![4],
                1 => child.session = 2,
                2 => child.elevated = true,
                3 => child.parent = 99,
                4 => child.created = 5,
                5 => child.id = 99,
                _ => child.created = 0,
            }
            assert!(!verified_descendant(&root, 2, |id| (id == 2).then(|| child.clone())));
        }
        assert!(!verified_descendant(&root, 0, |_| None));
        assert!(!matches(0, 0));
        assert!(!verified_descendant(&root, 2, |_| None));
    }

    #[test]
    fn reused_parent_pids_cycles_and_unbounded_chains_refuse() {
        let root = identity(1, 0, 10);
        assert!(!verified_descendant(&root, 3, |id| match id {
            3 => Some(identity(3, 2, 20)),
            2 => Some(identity(2, 1, 30)),
            _ => None,
        }));
        assert!(!verified_descendant(&root, 2, |id| Some(identity(
            id,
            if id == 2 { 3 } else { 2 },
            20
        ))));
        assert!(!verified_descendant(&root, 100, |id| Some(identity(
            id,
            id - 1,
            id as u64 + 10
        ))));
        let mut unsafe_root = root.clone();
        unsafe_root.elevated = true;
        assert!(!verified_descendant(&unsafe_root, 2, |id| Some(identity(
            id, 1, 20
        ))));
    }
}
