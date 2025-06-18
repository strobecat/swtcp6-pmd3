use crate::device::{VirtualNIC, VirtualNICWrapper};
use crate::socket::TcpSocket;
use pyo3::{create_exception, exceptions::PyException, prelude::*};
use rand::random;
use smoltcp::{
    iface::{Config, Interface as SmoltcpInterface, SocketSet},
    socket::tcp::{Socket, SocketBuffer},
    time::Instant,
    wire::{HardwareAddress, Ipv6Address, Ipv6Cidr},
};
use std::str::FromStr;

create_exception!(swtcp6_pmd3, InvalidAddressError, PyException);
create_exception!(swtcp6_pmd3, ConnectError, PyException);
create_exception!(swtcp6_pmd3, ListenError, PyException);

#[pyclass(module = "swtcp6_pmd3")]
pub struct Interface {
    intf: SmoltcpInterface,
    device: VirtualNICWrapper,
    pub(crate) sockets: SocketSet<'static>,
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

    fn listen(mut slf: PyRefMut<'_, Self>, port: u16) -> PyResult<TcpSocket> {
        let sock = slf.py().allow_threads(|| {
            Socket::new(
                SocketBuffer::new(vec![0; 65535]),
                SocketBuffer::new(vec![0; 65535]),
            )
        });

        let intf = &mut *slf;
        let handle = intf.sockets.add(sock);
        let socket = intf.sockets.get_mut::<Socket>(handle);
        socket
            .listen(port)
            .map_err(|err| ListenError::new_err(err.to_string()))?;
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
