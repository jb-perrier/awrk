use std::{
	io,
	net::{TcpListener, TcpStream},
};

pub struct SessionListener {
	listener: TcpListener,
}

impl SessionListener {
	pub fn bind(port: u16) -> io::Result<Self> {
		let listener = TcpListener::bind(("127.0.0.1", port))?;
		listener.set_nonblocking(true)?;
		Ok(Self { listener })
	}

	pub fn accept(&self) -> io::Result<TcpStream> {
		let (stream, _addr) = self.listener.accept()?;
		Ok(stream)
	}
}
