use std::mem::swap;

use pyo3::{prelude::*, types::PyBytes};
use smoltcp::{
    phy::{self, Device, DeviceCapabilities, Medium},
    time::Instant,
};

#[pyclass(module = "swtcp6_pmd3")]
pub struct VirtualNIC {
    #[pyo3(get)]
    mtu: u32,
    rx_buffer: Vec<u8>,
    tx_buffer: Vec<u8>,
}

#[pymethods]
impl VirtualNIC {
    #[new]
    fn __new__(mtu: u32) -> Self {
        VirtualNIC {
            mtu,
            rx_buffer: Vec::with_capacity(mtu.try_into().unwrap()),
            tx_buffer: Vec::with_capacity(mtu.try_into().unwrap()),
        }
    }

    fn extend_rx_buffer(&mut self, data: &Bound<PyBytes>) {
        self.rx_buffer.extend_from_slice(data.as_bytes());
    }

    fn can_consume_tx_buffer(&self) -> bool {
        !self.tx_buffer.is_empty()
    }

    fn consume_tx_buffer<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        PyBytes::new_with(py, self.tx_buffer.len(), |buffer: &mut [u8]| {
            py.allow_threads(|| {
                buffer.copy_from_slice(&self.tx_buffer);
                self.tx_buffer.clear();
            });
            Ok(())
        })
    }
}

pub struct RxToken(Vec<u8>);

impl phy::RxToken for RxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.0[..])
    }
}

pub struct TxToken(Py<VirtualNIC>);

impl phy::TxToken for TxToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buffer = vec![0; len];
        let result = f(&mut buffer);
        Python::with_gil(|py| {
            let nic = &mut *self.0.bind(py).borrow_mut();
            py.allow_threads(|| nic.tx_buffer.extend(buffer));
        });
        result
    }
}

pub(crate) struct VirtualNICWrapper(pub(crate) Py<VirtualNIC>);

impl Device for VirtualNICWrapper {
    type RxToken<'a>
        = RxToken
    where
        Self: 'a;
    type TxToken<'a>
        = TxToken
    where
        Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        Python::with_gil(|py| {
            let device = &mut *self.0.bind(py).borrow_mut();
            if device.rx_buffer.len() > 0 {
                let mut buffer = Vec::with_capacity(device.mtu.try_into().unwrap());
                py.allow_threads(|| {
                    swap(&mut device.rx_buffer, &mut buffer);
                });
                Some((RxToken(buffer), TxToken(self.0.clone_ref(py))))
            } else {
                None
            }
        })
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Python::with_gil(|py| Some(TxToken(self.0.clone_ref(py))))
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        Python::with_gil(|py| {
            let device = &mut *self.0.bind(py).borrow_mut();
            caps.max_transmission_unit = device.mtu.try_into().unwrap();
        });
        caps.medium = Medium::Ip;
        caps
    }
}
