use crate::device::{VirtualNIC, VirtualNICWrapper};
use pyo3::{create_exception, exceptions::PyException, prelude::*, types::PyBytes};
use rand::random;
use smoltcp::{
    iface::{Config, Interface as SmoltcpInterface, SocketHandle, SocketSet},
    socket::tcp::{Socket, SocketBuffer},
    time::Instant,
    wire::{HardwareAddress, Ipv6Address, Ipv6Cidr},
};
use std::str::FromStr;

create_exception!(swtcp6_pmd3, SendError, PyException);
create_exception!(swtcp6_pmd3, RecvError, PyException);

#[pyclass]
pub struct TcpSocket {
    handle: SocketHandle,
    intf: Py<Interface>,
}
#[pymethods]
impl TcpSocket {
    fn can_send(&self) -> bool {
        Python::with_gil(|py| {
            let intf = &*self.intf.borrow(py);
            let socket = intf.sockets.get::<Socket>(self.handle);
            socket.can_send()
        })
    }

    fn may_send(&self) -> bool {
        Python::with_gil(|py| {
            let intf = &*self.intf.borrow(py);
            let socket = intf.sockets.get::<Socket>(self.handle);
            socket.may_send()
        })
    }

    fn can_recv(&self) -> bool {
        Python::with_gil(|py| {
            let intf = &*self.intf.borrow(py);
            let socket = intf.sockets.get::<Socket>(self.handle);
            socket.can_recv()
        })
    }

    fn may_recv(&self) -> bool {
        Python::with_gil(|py| {
            let intf = &*self.intf.borrow(py);
            let socket = intf.sockets.get::<Socket>(self.handle);
            socket.may_recv()
        })
    }

    fn send_buf_available(&self) -> usize {
        Python::with_gil(|py| {
            let intf = &*self.intf.borrow(py);
            let socket = intf.sockets.get::<Socket>(self.handle);
            socket.send_capacity() - socket.send_queue()
        })
    }

    fn send(&mut self, data: &[u8]) -> PyResult<usize> {
        Python::with_gil(|py| {
            let intf = &mut *self.intf.borrow_mut(py);
            let socket = intf.sockets.get_mut::<Socket>(self.handle);
            py.allow_threads(|| {
                socket
                    .send_slice(data)
                    .map_err(|err| SendError::new_err(err.to_string()))
            })
        })
    }

    fn recv(&mut self) -> PyResult<Py<PyBytes>> {
        Python::with_gil(|py| {
            let intf = &mut *self.intf.borrow_mut(py);
            let socket = intf.sockets.get_mut::<Socket>(self.handle);
            PyBytes::new_with(py, socket.recv_queue(), |buffer: &mut [u8]| {
                py.allow_threads(|| match socket.recv_slice(buffer) {
                    Ok(_) => Ok(()),
                    Err(err) => Err(RecvError::new_err(err.to_string())),
                })
            })
            .map(|obj| obj.unbind())
        })
    }

    fn close(&mut self) {
        Python::with_gil(|py| {
            let intf = &mut *self.intf.borrow_mut(py);
            let socket = intf.sockets.get_mut::<Socket>(self.handle);
            socket.close();
        })
    }
}

create_exception!(swtcp6_pmd3, InvalidAddressError, PyException);
create_exception!(swtcp6_pmd3, ConnectError, PyException);

#[pyclass]
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
    fn device(&self) -> Py<VirtualNIC> {
        Python::with_gil(|py| self.device.0.clone_ref(py))
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
        let sock = Socket::new(
            SocketBuffer::new(vec![0; 65535]),
            SocketBuffer::new(vec![0; 65535]),
        );

        let intf = &mut *slf;
        let dest_ip = Ipv6Address::from_str(&ip)
            .map_err(|err| InvalidAddressError::new_err(err.to_string()))?;
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
}
