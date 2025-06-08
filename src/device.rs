use std::{cell::RefCell, mem::swap};

use pyo3::{prelude::*, types::PyBytes};
use smoltcp::{
    phy::{self, Device, DeviceCapabilities, Medium},
    time::Instant,
};

#[pyclass]
pub struct VirtualNIC {
    #[pyo3(get)]
    mtu: u32,
    received_buffer: Vec<u8>,
    consumer: PyObject,
}

#[pymethods]
impl VirtualNIC {
    #[new]
    fn __new__(mtu: u32, consumer: PyObject) -> Self {
        VirtualNIC {
            mtu,
            received_buffer: Vec::with_capacity(mtu.try_into().unwrap()),
            consumer,
        }
    }

    fn data_received(&mut self, data: &Bound<'_, PyBytes>) {
        self.received_buffer.extend_from_slice(data.as_bytes());
    }
}

pub struct RxToken {
    buffer: Vec<u8>,
}

impl phy::RxToken for RxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.buffer[..])
    }
}

pub struct TxToken {
    consumer: PyObject,
}

impl phy::TxToken for TxToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        // unable to raise exception here, so just unwrap
        Python::with_gil(|py| {
            let result: RefCell<Option<R>> = RefCell::new(None);
            let bytes = PyBytes::new_with(py, len, |buffer: &mut [u8]| {
                result.replace(Some(f(buffer)));
                Ok(())
            })
            .unwrap();
            self.consumer.call1(py, (bytes,)).unwrap();
            result.take().unwrap()
        })
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
            if device.received_buffer.len() > 0 {
                let mut data = Vec::with_capacity(device.mtu.try_into().unwrap());
                py.allow_threads(|| {
                    swap(&mut device.received_buffer, &mut data);
                });
                Some((
                    RxToken { buffer: data },
                    TxToken {
                        consumer: Py::clone_ref(&device.consumer, py),
                    },
                ))
            } else {
                None
            }
        })
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Python::with_gil(|py| {
            Some(TxToken {
                consumer: {
                    let device = &mut *self.0.bind(py).borrow_mut();
                    Py::clone_ref(&device.consumer, py)
                },
            })
        })
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
