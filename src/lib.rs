use pyo3::prelude::*;

mod device;
mod intf;

#[pymodule]
fn swtcp6_pmd3(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<intf::Interface>()?;
    m.add_class::<intf::TcpState>()?;
    m.add_class::<intf::TcpSocket>()?;
    m.add_class::<device::VirtualNIC>()?;
    m.add("SendError", m.py().get_type::<intf::SendError>())?;
    m.add("RecvError", m.py().get_type::<intf::RecvError>())?;
    m.add(
        "InvalidAddressError",
        m.py().get_type::<intf::InvalidAddressError>(),
    )?;
    m.add("ConnectError", m.py().get_type::<intf::ConnectError>())?;
    #[cfg(debug_assertions)]
    pyo3_log::init();
    Ok(())
}
