#![forbid(unsafe_code)]

use pyo3::prelude::*;

#[pymodule]
mod zephyr_mcumgr {
    use pyo3::prelude::*;

    #[pyfunction] // Inline definition of a pyfunction, also made availlable to Python
    fn triple(x: usize) -> usize {
        x * 3
    }
}
