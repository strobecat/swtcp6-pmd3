use crate::intf::Interface;
use pyo3::{create_exception, exceptions::PyException, prelude::*, types::PyBytes};
use smoltcp::{
    iface::SocketHandle,
    socket::tcp::{Socket, State},
    time::Duration,
};

create_exception!(swtcp6_pmd3, SendError, PyException);
create_exception!(swtcp6_pmd3, RecvError, PyException);

#[pyclass(eq, module = "swtcp6_pmd3")]
#[derive(Debug, PartialEq)]
#[allow(non_camel_case_types)]
pub enum TcpState {
    CLOSED,
    LISTEN,
    SYN_SENT,
    SYN_RECEIVED,
    ESTABLISHED,
    FIN_WAIT_1,
    FIN_WAIT_2,
    CLOSE_WAIT,
    CLOSING,
    LAST_ACK,
    TIME_WAIT,
}
impl From<State> for TcpState {
    fn from(value: State) -> Self {
        match value {
            State::Closed => TcpState::CLOSED,
            State::Listen => TcpState::LISTEN,
            State::SynSent => TcpState::SYN_SENT,
            State::SynReceived => TcpState::SYN_RECEIVED,
            State::Established => TcpState::ESTABLISHED,
            State::FinWait1 => TcpState::FIN_WAIT_1,
            State::FinWait2 => TcpState::FIN_WAIT_2,
            State::CloseWait => TcpState::CLOSE_WAIT,
            State::Closing => TcpState::CLOSING,
            State::LastAck => TcpState::LAST_ACK,
            State::TimeWait => TcpState::TIME_WAIT,
        }
    }
}

#[pyclass(module = "swtcp6_pmd3")]
pub struct TcpSocket {
    pub(crate) handle: SocketHandle,
    pub(crate) intf: Py<Interface>,
}

#[pymethods]
impl TcpSocket {
    fn can_send(&self, py: Python<'_>) -> bool {
        let intf = &*self.intf.borrow(py);
        let socket = intf.sockets.get::<Socket>(self.handle);
        socket.can_send()
    }

    fn may_send(&self, py: Python<'_>) -> bool {
        let intf = &*self.intf.borrow(py);
        let socket = intf.sockets.get::<Socket>(self.handle);
        socket.may_send()
    }

    fn can_recv(&self, py: Python<'_>) -> bool {
        let intf = &*self.intf.borrow(py);
        let socket = intf.sockets.get::<Socket>(self.handle);
        socket.can_recv()
    }

    fn may_recv(&self, py: Python<'_>) -> bool {
        let intf = &*self.intf.borrow(py);
        let socket = intf.sockets.get::<Socket>(self.handle);
        socket.may_recv()
    }

    fn state(&self, py: Python<'_>) -> TcpState {
        let intf = &*self.intf.borrow(py);
        let socket = intf.sockets.get::<Socket>(self.handle);
        socket.state().into()
    }

    fn send_buf_available(&self, py: Python<'_>) -> usize {
        let intf = &*self.intf.borrow(py);
        let socket = intf.sockets.get::<Socket>(self.handle);
        socket.send_capacity() - socket.send_queue()
    }

    fn send(&mut self, py: Python<'_>, data: &[u8]) -> PyResult<usize> {
        let intf = &mut *self.intf.borrow_mut(py);
        let socket = intf.sockets.get_mut::<Socket>(self.handle);
        py.allow_threads(|| {
            socket
                .send_slice(data)
                .map_err(|err| SendError::new_err(err.to_string()))
        })
    }

    fn recv<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let intf = &mut *self.intf.borrow_mut(py);
        let socket = intf.sockets.get_mut::<Socket>(self.handle);
        PyBytes::new_with(py, socket.recv_queue(), |buffer: &mut [u8]| {
            py.allow_threads(|| match socket.recv_slice(buffer) {
                Ok(_) => Ok(()),
                Err(err) => Err(RecvError::new_err(err.to_string())),
            })
        })
    }

    fn close(&mut self, py: Python<'_>) {
        let intf = &mut *self.intf.borrow_mut(py);
        let socket = intf.sockets.get_mut::<Socket>(self.handle);
        socket.close();
    }

    fn abort(&mut self, py: Python<'_>) {
        let intf = &mut *self.intf.borrow_mut(py);
        let socket = intf.sockets.get_mut::<Socket>(self.handle);
        socket.abort();
    }

    #[getter]
    fn get_keep_alive(&self, py: Python<'_>) -> Option<u64> {
        let intf = &*self.intf.borrow_mut(py);
        let socket = intf.sockets.get::<Socket>(self.handle);
        socket.keep_alive().map(|duration| duration.secs())
    }

    #[setter]
    fn set_keep_alive(&mut self, py: Python<'_>, secs: Option<u64>) {
        let intf = &mut *self.intf.borrow_mut(py);
        let socket = intf.sockets.get_mut::<Socket>(self.handle);
        socket.set_keep_alive(secs.map(|secs| Duration::from_secs(secs)));
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        let intf = &*self.intf.borrow(py);
        let socket = intf.sockets.get::<Socket>(self.handle);
        let laddr = socket.local_endpoint().unwrap();
        let raddr = socket.remote_endpoint().unwrap();
        let state: TcpState = socket.state().into();
        format!(
            "<swtcp6_pmd3.TcpSocket laddr=('{}', {}), raddr=('{}', {}), state=TcpState.{:?}>",
            laddr.addr, laddr.port, raddr.addr, raddr.port, state,
        )
    }
}

impl Drop for TcpSocket {
    fn drop(&mut self) {
        Python::with_gil(|py| {
            let intf = &mut *self.intf.borrow_mut(py);
            py.allow_threads(|| intf.sockets.remove(self.handle));
        });
    }
}
