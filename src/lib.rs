use pyo3::prelude::*;

mod device;
mod intf;
mod socket;

#[pymodule]
fn swtcp6_pmd3(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<device::VirtualNIC>()?;
    m.add_class::<intf::Interface>()?;
    m.add_class::<socket::TcpState>()?;
    m.add_class::<socket::TcpSocket>()?;
    m.add("SendError", m.py().get_type::<socket::SendError>())?;
    m.add("RecvError", m.py().get_type::<socket::RecvError>())?;
    m.add(
        "InvalidAddressError",
        m.py().get_type::<intf::InvalidAddressError>(),
    )?;
    m.add("ConnectError", m.py().get_type::<intf::ConnectError>())?;
    #[cfg(debug_assertions)]
    pyo3_log::init();
    Ok(())
}
