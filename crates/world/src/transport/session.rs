use std::{
	collections::{HashMap, VecDeque},
	io::{Read, Write},
	net::TcpStream,
};

pub const WORLD_MAX_FRAME_SIZE: usize = 8 * 1024 * 1024;

pub struct Session {
	stream: TcpStream,
	rx_buf: Vec<u8>,
	tx_buf: Vec<u8>,
	eof: bool,

	invoke_result_cache: HashMap<u64, Vec<u8>>,
	invoke_result_order: VecDeque<u64>,
}

impl Session {
	pub fn new(stream: TcpStream) -> Self {
		let _ = stream.set_nonblocking(true);
		let _ = stream.set_nodelay(true);
		Self {
			stream,
			rx_buf: Vec::new(),
			tx_buf: Vec::new(),
			eof: false,
			invoke_result_cache: HashMap::new(),
			invoke_result_order: VecDeque::new(),
		}
	}

	pub fn cache_invoke_result(&mut self, id: u64, payload: Vec<u8>) {
		const MAX_ENTRIES: usize = 256;

		if self.invoke_result_cache.contains_key(&id) {
			return;
		}

		self.invoke_result_cache.insert(id, payload);
		self.invoke_result_order.push_back(id);

		while self.invoke_result_order.len() > MAX_ENTRIES {
			if let Some(old) = self.invoke_result_order.pop_front() {
				self.invoke_result_cache.remove(&old);
			}
		}
	}

	pub fn cloned_cached_invoke_result(&self, id: u64) -> Option<Vec<u8>> {
		self.invoke_result_cache.get(&id).cloned()
	}

	pub fn poll_read(&mut self) -> std::io::Result<bool> {
		if self.eof {
			return Ok(true);
		}

		let mut tmp = [0u8; 4096];
		loop {
			match self.stream.read(&mut tmp) {
				Ok(0) => {
					self.eof = true;
					break;
				}
				Ok(n) => self.rx_buf.extend_from_slice(&tmp[..n]),
				Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
				Err(e) => return Err(e),
			}
		}

		Ok(self.eof)
	}

	pub fn try_read_frame(&mut self, max_frame_size: usize) -> std::io::Result<Option<Vec<u8>>> {
		if self.rx_buf.len() < 4 {
			return Ok(None);
		}

		let len = u32::from_le_bytes(self.rx_buf[..4].try_into().unwrap()) as usize;
		if len > max_frame_size {
			return Err(std::io::Error::new(
				std::io::ErrorKind::InvalidData,
				format!("Frame too large: {len} bytes (max {max_frame_size})"),
			));
		}

		let total = 4 + len;
		if self.rx_buf.len() < total {
			return Ok(None);
		}

		let payload = self.rx_buf[4..total].to_vec();
		self.rx_buf.drain(..total);
		Ok(Some(payload))
	}

	pub fn has_pending_rx(&self) -> bool {
		!self.rx_buf.is_empty()
	}

	pub fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
		let payload_len = buf.len();
		if payload_len > u32::MAX as usize {
			return Err(std::io::Error::new(
				std::io::ErrorKind::InvalidInput,
				"Message too large",
			));
		}

		let len = payload_len as u32;
		self.tx_buf.extend_from_slice(&len.to_le_bytes());
		self.tx_buf.extend_from_slice(buf);

		self.flush_write()?;
		Ok(payload_len)
	}

	pub fn flush_write(&mut self) -> std::io::Result<()> {
		while !self.tx_buf.is_empty() {
			match self.stream.write(&self.tx_buf) {
				Ok(0) => {
					return Err(std::io::Error::new(
						std::io::ErrorKind::WriteZero,
						"Failed to write to stream",
					));
				}
				Ok(n) => {
					self.tx_buf.drain(..n);
				}
				Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
				Err(e) => return Err(e),
			}
		}
		Ok(())
	}
}
