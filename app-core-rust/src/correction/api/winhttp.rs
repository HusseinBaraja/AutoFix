use std::{
    ffi::c_void,
    ptr::{null, null_mut},
    time::Duration,
};

use windows_sys::Win32::{
    Foundation::GetLastError,
    Networking::WinHttp::{
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
        WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetOption,
        WinHttpSetTimeouts, ERROR_WINHTTP_TIMEOUT, WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
        WINHTTP_DISABLE_REDIRECTS, WINHTTP_FLAG_SECURE, WINHTTP_OPTION_DISABLE_FEATURE,
        WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
    },
};

use super::{failure_error, invalid, ApiError, EngineFailureKind, MAX_RESPONSE_BYTES};

struct Endpoint<'a> {
    secure: bool,
    host: &'a str,
    port: u16,
    path: &'a str,
}

/// Validates a provider URL without opening a network connection.
pub(super) fn validate_base(base: &str) -> Result<(), ApiError> {
    parse(base).map(|_| ())
}

/// Parses the endpoint and restricts cleartext HTTP to loopback hosts.
fn parse(url: &str) -> Result<Endpoint<'_>, ApiError> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| invalid("Invalid API base URL"))?;
    let secure = scheme == "https";
    if !secure && scheme != "http" {
        return Err(invalid("Invalid API base URL scheme"));
    }
    let (authority, path) = rest
        .find('/')
        .map_or((rest, "/"), |at| (&rest[..at], &rest[at..]));
    if authority.is_empty()
        || authority.contains(['@', '?', '#', '\\'])
        || path.contains(['?', '#', '\\'])
    {
        return Err(invalid("Invalid API base URL"));
    }
    let (host, port) = if authority.starts_with('[') {
        let end = authority
            .find(']')
            .ok_or_else(|| invalid("Invalid API host"))?;
        let host = &authority[..=end];
        let suffix = &authority[end + 1..];
        let port = if suffix.is_empty() {
            None
        } else {
            Some(
                suffix
                    .strip_prefix(':')
                    .ok_or_else(|| invalid("Invalid API port"))?,
            )
        };
        (host, port)
    } else {
        authority
            .split_once(':')
            .map_or((authority, None), |(host, port)| (host, Some(port)))
    };
    if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(invalid("Invalid API host"));
    }
    if !secure && !matches!(host, "localhost" | "127.0.0.1" | "[::1]") {
        return Err(invalid("HTTP API endpoints must use loopback"));
    }
    let port = port
        .map(|value| {
            value
                .parse::<u16>()
                .map_err(|_| invalid("Invalid API port"))
        })
        .transpose()?
        .unwrap_or(if secure { 443 } else { 80 });
    if port == 0 {
        return Err(invalid("Invalid API port"));
    }
    Ok(Endpoint {
        secure,
        host,
        port,
        path,
    })
}

struct Handle(*mut c_void);
impl Drop for Handle {
    /// Releases the owned WinHTTP handle on every exit path.
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }
}

/// Encodes a null-terminated UTF-16 string for WinHTTP.
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Sends JSON without redirects and caps the response size and request time.
pub(super) fn post(
    url: &str,
    key: &str,
    body: &str,
    timeout: Duration,
) -> Result<String, ApiError> {
    let endpoint = parse(url)?;
    let timeout_ms = timeout.as_millis().clamp(1, i32::MAX as u128) as i32;
    let agent = wide("AutoFix/1.0");
    let session = Handle(unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            null(),
            null(),
            0,
        )
    });
    if session.0.is_null() {
        return Err(last_error());
    }
    if unsafe { WinHttpSetTimeouts(session.0, timeout_ms, timeout_ms, timeout_ms, timeout_ms) } == 0
    {
        return Err(last_error());
    }
    let host = wide(endpoint.host.trim_matches(['[', ']']));
    let connection = Handle(unsafe { WinHttpConnect(session.0, host.as_ptr(), endpoint.port, 0) });
    if connection.0.is_null() {
        return Err(last_error());
    }
    let verb = wide("POST");
    let path = wide(endpoint.path);
    let request = Handle(unsafe {
        WinHttpOpenRequest(
            connection.0,
            verb.as_ptr(),
            path.as_ptr(),
            null(),
            null(),
            null(),
            if endpoint.secure {
                WINHTTP_FLAG_SECURE
            } else {
                0
            },
        )
    });
    if request.0.is_null() {
        return Err(last_error());
    }
    let disabled: u32 = WINHTTP_DISABLE_REDIRECTS;
    if unsafe {
        WinHttpSetOption(
            request.0,
            WINHTTP_OPTION_DISABLE_FEATURE,
            &disabled as *const _ as _,
            4,
        )
    } == 0
    {
        return Err(last_error());
    }
    let headers = wide(&format!(
        "Content-Type: application/json\r\nAuthorization: Bearer {key}\r\n"
    ));
    let body_len = u32::try_from(body.len()).map_err(|_| invalid("API request is too large"))?;
    if unsafe {
        WinHttpSendRequest(
            request.0,
            headers.as_ptr(),
            u32::MAX,
            body.as_ptr().cast(),
            body_len,
            body_len,
            0,
        )
    } == 0
    {
        return Err(last_error());
    }
    if unsafe { WinHttpReceiveResponse(request.0, null_mut()) } == 0 {
        return Err(last_error());
    }
    let mut status: u32 = 0;
    let mut status_size = 4;
    if unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            &mut status as *mut _ as _,
            &mut status_size,
            null_mut(),
        )
    } == 0
    {
        return Err(last_error());
    }
    match status {
        200..=299 => {}
        401 | 403 => {
            return Err(failure_error(
                EngineFailureKind::Authentication,
                "API authentication failed",
                false,
            ))
        }
        429 => {
            return Err(failure_error(
                EngineFailureKind::RateLimited,
                "API rate limit reached",
                true,
            ))
        }
        500..=599 => {
            return Err(failure_error(
                EngineFailureKind::Transport,
                "API service unavailable",
                true,
            ))
        }
        _ => {
            return Err(failure_error(
                EngineFailureKind::InvalidInput,
                "API rejected request",
                false,
            ))
        }
    }
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0u8; 8192];
        let mut read = 0;
        if unsafe {
            WinHttpReadData(
                request.0,
                chunk.as_mut_ptr().cast(),
                chunk.len() as u32,
                &mut read,
            )
        } == 0
        {
            return Err(last_error());
        }
        if read == 0 {
            break;
        }
        if bytes.len() + read as usize > MAX_RESPONSE_BYTES as usize {
            return Err(failure_error(
                EngineFailureKind::InvalidResponse,
                "API response is too large",
                false,
            ));
        }
        bytes.extend_from_slice(&chunk[..read as usize]);
    }
    String::from_utf8(bytes).map_err(|_| {
        failure_error(
            EngineFailureKind::InvalidResponse,
            "API response is not UTF-8",
            false,
        )
    })
}

/// Maps WinHTTP timeout separately from retryable transport failures.
fn last_error() -> ApiError {
    match unsafe { GetLastError() } {
        ERROR_WINHTTP_TIMEOUT => ApiError::Timeout,
        _ => failure_error(EngineFailureKind::Transport, "API transport failed", true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn posts_json_and_reads_openai_compatible_response() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            let header_end = loop {
                let count = stream.read(&mut buffer).unwrap();
                request.extend_from_slice(&buffer[..count]);
                if let Some(at) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    break at + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request);
            assert!(headers.starts_with("POST /v1/chat/completions HTTP/1.1"));
            assert!(headers.contains("Authorization: Bearer test-key"));
            let body_len: usize = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|len| len.trim().parse().ok())
                })
                .unwrap();
            while request.len() - header_end < body_len {
                let count = stream.read(&mut buffer).unwrap();
                request.extend_from_slice(&buffer[..count]);
            }
            let body = r#"{"choices":[{"message":{"content":"{\"corrected_executable_text\":\"the\"}"}}]}"#;
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        });
        let body = post(
            &format!("http://127.0.0.1:{port}/v1/chat/completions"),
            "test-key",
            "{}",
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(body.contains("corrected_executable_text"));
        server.join().unwrap();
    }
}
