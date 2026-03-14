use crate::rpc::{
    GetEntitiesArgs, GetEntitiesResult, ListEntitiesResult, ListTypesResult, PollChangesArgs,
    PollChangesResult, QueryEntitiesArgs, QueryEntitiesResult,
};
use crate::transport::WORLD_MAX_FRAME_SIZE;
use awrk_datex::codec::decode::DecodeConfig;
use awrk_datex::{Decode, Encode};
use awrk_datex_rpc::{RpcEnvelopeRef, RpcProcId, decode_envelope, encode_invoke};
use std::fmt;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct RpcTrace {
    pub at: Instant,
    pub id: u64,
    pub proc: String,
    pub duration_us: u64,
    pub request_bytes: u64,
    pub response_bytes: Option<u64>,
    pub response_decode_us: Option<u64>,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct WorldClientOptions {
    pub connect_timeout: Duration,
    pub io_timeout: Duration,
    pub nodelay: bool,
}

impl Default for WorldClientOptions {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_millis(500),
            io_timeout: Duration::from_secs(5),
            nodelay: true,
        }
    }
}

#[derive(Debug)]
pub enum WorldClientError {
    Transport(String),
    Protocol(String),
    Rpc(String),
}

impl WorldClientError {
    pub fn is_transport(&self) -> bool {
        matches!(self, Self::Transport(_))
    }
}

impl fmt::Display for WorldClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(s) => write!(f, "transport error: {s}"),
            Self::Protocol(s) => write!(f, "protocol error: {s}"),
            Self::Rpc(s) => write!(f, "rpc error: {s}"),
        }
    }
}

impl std::error::Error for WorldClientError {}

pub struct WorldClient {
    stream: TcpStream,
    next_id: u64,
    host: String,
    port: u16,
    opts: WorldClientOptions,
    traces: Vec<RpcTrace>,
}

impl WorldClient {
    pub fn connect(host: &str, port: u16, opts: WorldClientOptions) -> Result<Self, String> {
        let addrs = (host, port)
            .to_socket_addrs()
            .map_err(|e| e.to_string())?
            .collect::<Vec<_>>();

        let addr = addrs
            .into_iter()
            .next()
            .ok_or_else(|| "no socket addresses".to_string())?;

        let stream = TcpStream::connect_timeout(&addr, opts.connect_timeout)
            .map_err(|e| format!("connect failed: {e}"))?;

        let _ = stream.set_nodelay(opts.nodelay);
        let _ = stream.set_read_timeout(Some(opts.io_timeout));
        let _ = stream.set_write_timeout(Some(opts.io_timeout));

        Ok(Self {
            stream,
            next_id: 1,
            host: host.to_string(),
            port,
            opts,
            traces: Vec::new(),
        })
    }

    fn reconnect(&mut self) -> Result<(), std::io::Error> {
        let addrs = (self.host.as_str(), self.port)
            .to_socket_addrs()?
            .collect::<Vec<_>>();

        let addr = addrs
            .into_iter()
            .next()
            .ok_or_else(|| std::io::Error::other("no socket addresses"))?;

        let stream = TcpStream::connect_timeout(&addr, self.opts.connect_timeout)?;
        let _ = stream.set_nodelay(self.opts.nodelay);
        let _ = stream.set_read_timeout(Some(self.opts.io_timeout));
        let _ = stream.set_write_timeout(Some(self.opts.io_timeout));

        self.stream = stream;
        Ok(())
    }

    pub fn traces(&self) -> &[RpcTrace] {
        &self.traces
    }

    pub fn get_schema_bytes(&mut self) -> Result<Vec<u8>, WorldClientError> {
        self.invoke_typed::<(), Vec<u8>>("awrk.get_schema", ())
    }

    pub fn list_types(&mut self) -> Result<ListTypesResult, WorldClientError> {
        self.invoke_typed::<(), ListTypesResult>("awrk.list_types", ())
    }

    pub fn list_entities(&mut self) -> Result<ListEntitiesResult, WorldClientError> {
        self.invoke_typed::<(), ListEntitiesResult>("awrk.list_entities", ())
    }

    pub fn query_entities(
        &mut self,
        args: QueryEntitiesArgs,
    ) -> Result<QueryEntitiesResult, WorldClientError> {
        self.invoke_typed::<QueryEntitiesArgs, QueryEntitiesResult>("awrk.query_entities", args)
    }

    pub fn get_entities(
        &mut self,
        entities: Vec<u64>,
    ) -> Result<GetEntitiesResult, WorldClientError> {
        self.invoke_typed::<GetEntitiesArgs, GetEntitiesResult>(
            "awrk.get_entities",
            GetEntitiesArgs { entities },
        )
    }

    pub fn poll_changes(
        &mut self,
        since: u64,
        limit: Option<u32>,
    ) -> Result<PollChangesResult, WorldClientError> {
        self.invoke_typed::<PollChangesArgs, PollChangesResult>(
            "awrk.poll_changes",
            PollChangesArgs { since, limit },
        )
    }

    pub fn invoke_value(
        &mut self,
        proc: &str,
        args: awrk_datex::value::Value,
    ) -> Result<awrk_datex::value::Value, WorldClientError> {
        let start = Instant::now();
        let id = self.next_message_id();

        let proc_id = RpcProcId(proc_id_for_name(proc));
        let payload = encode_invoke(id, proc_id, |enc| args.wire_encode(enc)).map_err(|e| {
            self.traces.push(RpcTrace {
                at: Instant::now(),
                id,
                proc: proc.to_string(),
                duration_us: start.elapsed().as_micros() as u64,
                request_bytes: 0,
                response_bytes: None,
                response_decode_us: None,
                ok: false,
                error: Some(format!("protocol error: {e}")),
            });
            WorldClientError::Protocol(e.to_string())
        })?;

        let request_bytes = (4 + payload.len()) as u64;

        if let Err(e) = write_frame(&mut self.stream, &payload) {
            if is_reconnectable_io_error(&e) {
                if self.reconnect().is_ok() {
                    write_frame(&mut self.stream, &payload).map_err(|e| {
                        self.traces.push(RpcTrace {
                            at: Instant::now(),
                            id,
                            proc: proc.to_string(),
                            duration_us: start.elapsed().as_micros() as u64,
                            request_bytes,
                            response_bytes: None,
                            response_decode_us: None,
                            ok: false,
                            error: Some(format!("transport error: {e}")),
                        });
                        WorldClientError::Transport(e.to_string())
                    })?;
                } else {
                    self.traces.push(RpcTrace {
                        at: Instant::now(),
                        id,
                        proc: proc.to_string(),
                        duration_us: start.elapsed().as_micros() as u64,
                        request_bytes,
                        response_bytes: None,
                        response_decode_us: None,
                        ok: false,
                        error: Some(format!("transport error: {e}")),
                    });
                    return Err(WorldClientError::Transport(e.to_string()));
                }
            } else {
                self.traces.push(RpcTrace {
                    at: Instant::now(),
                    id,
                    proc: proc.to_string(),
                    duration_us: start.elapsed().as_micros() as u64,
                    request_bytes,
                    response_bytes: None,
                    response_decode_us: None,
                    ok: false,
                    error: Some(format!("transport error: {e}")),
                });
                return Err(WorldClientError::Transport(e.to_string()));
            }
        }

        let resp_bytes = match read_frame(&mut self.stream) {
            Ok(bytes) => bytes,
            Err(e) => {
                if is_reconnectable_io_error(&e) {
                    let _ = self.reconnect();
                }
                self.traces.push(RpcTrace {
                    at: Instant::now(),
                    id,
                    proc: proc.to_string(),
                    duration_us: start.elapsed().as_micros() as u64,
                    request_bytes,
                    response_bytes: None,
                    response_decode_us: None,
                    ok: false,
                    error: Some(format!("transport error: {e}")),
                });
                return Err(WorldClientError::Transport(e.to_string()));
            }
        };

        let response_bytes = Some((4 + resp_bytes.len()) as u64);

        let decode_start = Instant::now();
        let env = decode_envelope(&resp_bytes, DecodeConfig::default()).map_err(|e| {
            self.traces.push(RpcTrace {
                at: Instant::now(),
                id,
                proc: proc.to_string(),
                duration_us: start.elapsed().as_micros() as u64,
                request_bytes,
                response_bytes,
                response_decode_us: Some(decode_start.elapsed().as_micros() as u64),
                ok: false,
                error: Some(format!("protocol error: {e}")),
            });
            WorldClientError::Protocol(e.to_string())
        })?;
        let response_decode_us = Some(decode_start.elapsed().as_micros() as u64);

        let duration_us_after_decode = start.elapsed().as_micros() as u64;

        match env {
            RpcEnvelopeRef::InvokeResult {
                id: resp_id,
                ok,
                value,
            } => {
                if resp_id != id {
                    let err = format!("response id mismatch: expected {id}, got {resp_id}");
                    self.traces.push(RpcTrace {
                        at: Instant::now(),
                        id,
                        proc: proc.to_string(),
                        duration_us: duration_us_after_decode,
                        request_bytes,
                        response_bytes,
                        response_decode_us,
                        ok: false,
                        error: Some(format!("protocol error: {err}")),
                    });
                    return Err(WorldClientError::Protocol(err));
                }

                if ok {
                    let v = awrk_datex::value::Value::wire_decode(value).map_err(|e| {
                        let err = e.to_string();
                        self.traces.push(RpcTrace {
                            at: Instant::now(),
                            id,
                            proc: proc.to_string(),
                            duration_us: duration_us_after_decode,
                            request_bytes,
                            response_bytes,
                            response_decode_us,
                            ok: false,
                            error: Some(format!("protocol error: {err}")),
                        });
                        WorldClientError::Protocol(err)
                    })?;
                    self.traces.push(RpcTrace {
                        at: Instant::now(),
                        id,
                        proc: proc.to_string(),
                        duration_us: start.elapsed().as_micros() as u64,
                        request_bytes,
                        response_bytes,
                        response_decode_us,
                        ok: true,
                        error: None,
                    });
                    Ok(v)
                } else {
                    let msg = String::wire_decode(value)
                        .map_err(|e| WorldClientError::Protocol(e.to_string()))?;
                    self.traces.push(RpcTrace {
                        at: Instant::now(),
                        id,
                        proc: proc.to_string(),
                        duration_us: duration_us_after_decode,
                        request_bytes,
                        response_bytes,
                        response_decode_us,
                        ok: false,
                        error: Some(msg.clone()),
                    });
                    Err(WorldClientError::Rpc(msg))
                }
            }
            RpcEnvelopeRef::Invoke { .. } => {
                let err = "expected invoke_result".to_string();
                self.traces.push(RpcTrace {
                    at: Instant::now(),
                    id,
                    proc: proc.to_string(),
                    duration_us: duration_us_after_decode,
                    request_bytes,
                    response_bytes,
                    response_decode_us,
                    ok: false,
                    error: Some(format!("protocol error: {err}")),
                });
                Err(WorldClientError::Protocol(err))
            }
        }
    }

    pub fn invoke_typed<A, R>(&mut self, proc: &str, args: A) -> Result<R, WorldClientError>
    where
        A: Encode,
        for<'a> R: Decode<'a>,
    {
        let args_value = encode_typed_to_value(&args)?;
        let v = self.invoke_value(proc, args_value)?;
        decode_typed_from_value::<R>(&v)
    }

    fn next_message_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        id
    }
}

fn proc_id_for_name(name: &str) -> u64 {
    if name == "awrk.get_schema" {
        awrk_datex_schema::PROC_ID_GET_SCHEMA.0
    } else {
        awrk_datex_schema::proc_id(name).0
    }
}

fn encode_typed_to_value<A: Encode>(
    args: &A,
) -> Result<awrk_datex::value::Value, WorldClientError> {
    let mut enc = awrk_datex::codec::encode::Encoder::default();
    args.wire_encode(&mut enc)
        .map_err(|e| WorldClientError::Protocol(e.to_string()))?;
    let buf = enc.into_inner();
    let value_ref = awrk_datex::codec::decode::decode_value_full(&buf, DecodeConfig::default())
        .map_err(|e| WorldClientError::Protocol(e.to_string()))?;
    awrk_datex::value::Value::wire_decode(value_ref)
        .map_err(|e| WorldClientError::Protocol(e.to_string()))
}

fn decode_typed_from_value<R>(value: &awrk_datex::value::Value) -> Result<R, WorldClientError>
where
    for<'de> R: Decode<'de>,
{
    let mut enc = awrk_datex::codec::encode::Encoder::default();
    value
        .wire_encode(&mut enc)
        .map_err(|e| WorldClientError::Protocol(e.to_string()))?;
    let buf = enc.into_inner();
    let value_ref = awrk_datex::codec::decode::decode_value_full(&buf, DecodeConfig::default())
        .map_err(|e| WorldClientError::Protocol(e.to_string()))?;
    R::wire_decode(value_ref).map_err(|e| WorldClientError::Protocol(e.to_string()))
}

fn is_reconnectable_io_error(e: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    matches!(
        e.kind(),
        ErrorKind::BrokenPipe
            | ErrorKind::ConnectionAborted
            | ErrorKind::ConnectionReset
            | ErrorKind::NotConnected
            | ErrorKind::UnexpectedEof
            | ErrorKind::TimedOut
    )
}

fn write_frame(stream: &mut TcpStream, payload: &[u8]) -> std::io::Result<()> {
    if payload.len() > u32::MAX as usize {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "payload too large",
        ));
    }

    let len = payload.len() as u32;
    stream.write_all(&len.to_le_bytes())?;
    stream.write_all(payload)?;
    stream.flush()?;
    Ok(())
}

fn read_frame(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;

    if len > WORLD_MAX_FRAME_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("frame too large: {len} bytes (max {WORLD_MAX_FRAME_SIZE})"),
        ));
    }

    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload)?;
    Ok(payload)
}
