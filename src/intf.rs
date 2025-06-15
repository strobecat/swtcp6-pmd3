use crate::device::{VirtualNIC, VirtualNICWrapper};
use pyo3::{create_exception, exceptions::PyException, prelude::*, types::PyBytes};
use rand::random;
use smoltcp::{
    iface::{Config, Interface as SmoltcpInterface, SocketHandle, SocketSet},
    socket::tcp::{Socket, SocketBuffer, State},
    time::Instant,
    wire::{HardwareAddress, Ipv6Address, Ipv6Cidr},
};
use std::str::FromStr;

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
    handle: SocketHandle,
    intf: Py<Interface>,
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

create_exception!(swtcp6_pmd3, InvalidAddressError, PyException);
create_exception!(swtcp6_pmd3, ConnectError, PyException);

#[pyclass(module = "swtcp6_pmd3")]
pub struct Interface {
    intf: SmoltcpInterface,
    device: VirtualNICWrapper,
    sockets: SocketSet<'static>,
}

#[pymethods]
impl Interface {
    #[new]
    fn __new__(device: Py<VirtualNIC>, ip: String, netmask: String) -> PyResult<Self> {
        let netmask: u8 = Ipv6Address::from_str(&netmask)
            .map_err(|err| InvalidAddressError::new_err(err.to_string()))?
            .to_bits()
            .count_ones()
            .try_into()
            .map_err(|_| InvalidAddressError::new_err("invalid netmask"))?;
        if netmask > 128 {
            return Err(InvalidAddressError::new_err("invalid netmask"));
        }
        let mut config = Config::new(HardwareAddress::Ip);
        config.random_seed = random();
        let mut device = VirtualNICWrapper(device);
        let mut intf = SmoltcpInterface::new(config, &mut device, Instant::now());
        let ipaddr = Ipv6Address::from_str(&ip)
            .map_err(|err| InvalidAddressError::new_err(err.to_string()))?;
        intf.update_ip_addrs(|ip_addrs| {
            // IFACE_MAX_ADDR_COUNT should >=1 so we just unwrap
            ip_addrs
                .push(Ipv6Cidr::new(ipaddr, netmask).into())
                .unwrap()
        });
        Ok(Interface {
            intf,
            device,
            sockets: SocketSet::new(Vec::new()),
        })
    }

    #[getter]
    fn device(&self, py: Python<'_>) -> Py<VirtualNIC> {
        self.device.0.clone_ref(py)
    }

    fn poll(&mut self) {
        self.intf
            .poll(Instant::now(), &mut self.device, &mut self.sockets);
    }

    fn poll_delay(&mut self) -> Option<u64> {
        self.intf
            .poll_delay(Instant::now(), &self.sockets)
            .map(|instant| instant.secs())
    }

    fn connect(mut slf: PyRefMut<'_, Self>, ip: String, port: u16) -> PyResult<TcpSocket> {
        let sock = slf.py().allow_threads(|| {
            Socket::new(
                SocketBuffer::new(vec![0; 65535]),
                SocketBuffer::new(vec![0; 65535]),
            )
        });

        let dest_ip = Ipv6Address::from_str(&ip)
            .map_err(|err| InvalidAddressError::new_err(err.to_string()))?;
        let intf = &mut *slf;
        let handle = intf.sockets.add(sock);
        let socket = intf.sockets.get_mut::<Socket>(handle);

        socket
            .connect(
                intf.intf.context(),
                (dest_ip, port),
                49152 + random::<u16>() % 16384,
            )
            .map_err(|err| ConnectError::new_err(err.to_string()))?;
        Ok(TcpSocket {
            handle,
            intf: slf.into(),
        })
    }

    fn __repr__(&self) -> String {
        let addr = self.intf.ip_addrs()[0];
        format!(
            "<swtcp6_pmd3.Interface ip='{}' prefixlen={} sockets={}>",
            addr.address(),
            addr.prefix_len(),
            self.sockets.iter().count()
        )
    }
}
