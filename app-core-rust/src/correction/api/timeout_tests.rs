use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::atomic::{AtomicU64, Ordering},
};

static PROFILE_ID: AtomicU64 = AtomicU64::new(0);

struct Provider {
    profile: String,
    config: ApiEngineConfig,
    server: Option<thread::JoinHandle<usize>>,
}

impl Provider {
    /// CI without a Windows logon session cannot create provider credentials.
    fn start(replies: Vec<(u16, Duration, String)>) -> Option<Self> {
        let profile = format!(
            "timeout-test-{}-{}",
            std::process::id(),
            PROFILE_ID.fetch_add(1, Ordering::Relaxed)
        );
        if let Err(error) = crate::secrets::set_secret(&profile, "test-key") {
            eprintln!("skipping provider test: credential store unavailable: {error}");
            return None;
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let mut count = 0;
            for (status, delay, body) in replies {
                let started = Instant::now();
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            assert!(
                                started.elapsed() < Duration::from_secs(3),
                                "missing API attempt"
                            );
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                let header_end = loop {
                    let read = stream.read(&mut buffer).unwrap();
                    assert_ne!(read, 0);
                    request.extend_from_slice(&buffer[..read]);
                    if let Some(at) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                        break at + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|length| length.trim().parse::<usize>().ok())
                    })
                    .unwrap();
                while request.len() - header_end < length {
                    let read = stream.read(&mut buffer).unwrap();
                    assert_ne!(read, 0);
                    request.extend_from_slice(&buffer[..read]);
                }
                count += 1;
                thread::sleep(delay);
                // Timing out may close the connection before the provider responds.
                let _ = write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
            count
        });
        Some(Self {
            config: ApiEngineConfig {
                provider_preset: profile.clone(),
                base_url: Some(format!("http://127.0.0.1:{port}/v1")),
                timeout_manual_ms: 150,
                timeout_auto_ms: 60,
                ..ApiEngineConfig::default()
            },
            profile,
            server: Some(server),
        })
    }

    /// Join the loopback provider and verify the exact number of transport attempts.
    fn finish(mut self, attempts: usize) {
        assert_eq!(self.server.take().unwrap().join().unwrap(), attempts);
    }
}

impl Drop for Provider {
    /// Report credential cleanup failures without panicking during test unwinding.
    fn drop(&mut self) {
        if let Err(error) = crate::secrets::delete_secret(&self.profile) {
            eprintln!("test credential cleanup failed: {error}");
        }
    }
}

/// Reuse the API contract fixture with the requested trigger budget.
fn input(trigger: TriggerType) -> CorrectionInput {
    super::tests::input(trigger)
}

/// Build a valid provider response containing no executable edits.
fn unchanged_response() -> String {
    super::tests::response("teh AutoFix", json!([]))
}

/// Late transport success cannot escape the deadline or hold up the next request.
#[test]
fn deadline_discards_late_success_and_does_not_delay_the_next_request() {
    let (release, wait) = mpsc::channel();
    let (done, finished) = mpsc::channel();
    let started = Instant::now();
    let outcome = bounded_request(Duration::from_millis(40), started, move || {
        wait.recv().unwrap();
        done.send(()).unwrap();
        Ok("late correction".into())
    });
    assert!(matches!(outcome, Err(ApiError::Timeout)));
    assert!(started.elapsed() < Duration::from_millis(400));
    let current = bounded_request(Duration::from_secs(1), Instant::now(), || {
        Ok("current".into())
    });
    assert_eq!(current.unwrap(), "current");
    release.send(()).unwrap();
    finished.recv_timeout(Duration::from_secs(1)).unwrap();
}

/// Manual and automatic deadlines produce feedback or opt-in local fallback as configured.
#[test]
fn stalled_api_uses_trigger_timeout_and_only_opt_in_fallback() {
    for trigger in [
        TriggerType::ManualShortcut,
        TriggerType::WordCount,
        TriggerType::Character,
    ] {
        for fallback in [false, true] {
            let Some(mut provider) = Provider::start(vec![(
                200,
                Duration::from_millis(350),
                unchanged_response(),
            )]) else {
                return;
            };
            provider.config.fallback_to_local = fallback;
            let request = input(trigger);
            let started = Instant::now();
            let output = ApiCorrectionEngine::new(EngineKind::CustomApi, provider.config.clone())
                .correct(&request);
            assert!(started.elapsed() < Duration::from_millis(300));
            if fallback {
                assert_eq!(output.status, EngineStatus::Completed);
                assert_eq!(output.corrected_executable_text, "the AutoFix");
                assert_eq!(
                    ApiCorrectionEngine::notice_for(&request, &output),
                    ApiNotice::None
                );
            } else {
                assert_eq!(output.status, EngineStatus::TimedOut);
                assert!(!output.changes_needed);
                assert_eq!(output.corrected_executable_text, request.executable_context);
                let expected = if trigger == TriggerType::ManualShortcut {
                    ApiNotice::ManualTimeout
                } else {
                    ApiNotice::None
                };
                assert_eq!(ApiCorrectionEngine::notice_for(&request, &output), expected);
            }
            provider.finish(1);
        }
    }
}

/// A retryable service error causes only the configured zero or one retry.
#[test]
fn configured_retry_count_controls_retryable_failures() {
    for retries in [0, 1] {
        let mut replies = vec![(503, Duration::ZERO, String::new())];
        if retries == 1 {
            replies.push((200, Duration::ZERO, unchanged_response()));
        }
        let Some(mut provider) = Provider::start(replies) else {
            return;
        };
        provider.config.retry_count = retries;
        provider.config.timeout_manual_ms = 3_000;
        let output = ApiCorrectionEngine::new(EngineKind::CustomApi, provider.config.clone())
            .correct(&input(TriggerType::ManualShortcut));
        if retries == 1 {
            assert_eq!(output.status, EngineStatus::Completed);
        } else {
            assert!(
                matches!(
                    output.status,
                    EngineStatus::Error(EngineFailure {
                        kind: EngineFailureKind::Transport,
                        ..
                    })
                ),
                "{output:?}"
            );
        }
        provider.finish(usize::from(retries) + 1);
    }
}

/// Retries share the first attempt deadline, and authentication errors never retry.
#[test]
fn retry_shares_deadline_and_authentication_is_not_retried() {
    let Some(provider) = Provider::start(vec![
        (503, Duration::from_millis(80), String::new()),
        (200, Duration::from_millis(200), unchanged_response()),
    ]) else {
        return;
    };
    let started = Instant::now();
    let output = ApiCorrectionEngine::new(EngineKind::CustomApi, provider.config.clone())
        .correct(&input(TriggerType::ManualShortcut));
    assert_eq!(output.status, EngineStatus::TimedOut);
    assert!(started.elapsed() < Duration::from_millis(260));
    provider.finish(2);

    let Some(provider) = Provider::start(vec![(401, Duration::ZERO, String::new())]) else {
        return;
    };
    let output = ApiCorrectionEngine::new(EngineKind::CustomApi, provider.config.clone())
        .correct(&input(TriggerType::ManualShortcut));
    assert!(matches!(
        output.status,
        EngineStatus::Error(EngineFailure {
            kind: EngineFailureKind::Authentication,
            ..
        })
    ));
    provider.finish(1);
}

/// Direct engine callers cannot bypass the supported retry-count range.
#[test]
fn direct_engine_config_rejects_retry_counts_above_one() {
    let config = ApiEngineConfig {
        retry_count: 2,
        ..ApiEngineConfig::default()
    };
    let output = ApiCorrectionEngine::new(EngineKind::CustomApi, config)
        .correct(&input(TriggerType::ManualShortcut));
    assert!(matches!(
        output.status,
        EngineStatus::Error(EngineFailure {
            kind: EngineFailureKind::InvalidInput,
            ..
        })
    ));
}

/// A retry needs fresh authorization even after the first attempt was permitted.
#[test]
fn revoked_send_authorization_prevents_api_retry() {
    let Some(mut provider) = Provider::start(vec![(503, Duration::ZERO, String::new())]) else {
        return;
    };
    provider.config.timeout_manual_ms = 1_000;
    let sends = Arc::new(AtomicU64::new(0));
    let attempts = Arc::clone(&sends);
    let authorize: SendAuthorization = Arc::new(move || {
        (attempts.fetch_add(1, Ordering::SeqCst) == 0).then(|| Box::new(()) as Box<dyn Send>)
    });
    let output = ApiCorrectionEngine::new(EngineKind::CustomApi, provider.config.clone())
        .with_send_authorization(authorize)
        .correct(&input(TriggerType::ManualShortcut));
    assert!(
        matches!(
            output.status,
            EngineStatus::Error(EngineFailure {
                kind: EngineFailureKind::InvalidInput,
                retryable: false,
                ..
            })
        ),
        "{output:?}"
    );
    assert_eq!(sends.load(Ordering::SeqCst), 2);
    provider.finish(1);
}
