mod client;
mod protocol;
mod server;
mod state;

#[cfg(test)]
mod tests;

pub(crate) use client::{send_request, IpcClientError};
pub(crate) use protocol::{IpcRequest, IpcResponse, UpdateSettingRequest};
#[cfg(test)]
pub(crate) use server::pipe_path_for_process;
pub(crate) use server::{NamedPipeIpcServer, PIPE_NAME};
pub(crate) use state::IpcServerState;
