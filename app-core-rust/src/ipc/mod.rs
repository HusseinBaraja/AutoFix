mod client;
mod protocol;
mod server;
mod state;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use client::{send_request, IpcClientError};
pub(crate) use protocol::IpcResponse;
#[cfg(test)]
pub(crate) use protocol::{IpcRequest, UpdateSettingRequest};
#[cfg(test)]
pub(crate) use server::pipe_path_for_process;
pub(crate) use server::NamedPipeIpcServer;
#[cfg(test)]
pub(crate) use server::PIPE_NAME;
pub(crate) use state::IpcServerState;
