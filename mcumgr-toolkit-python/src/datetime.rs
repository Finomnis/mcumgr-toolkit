//! Conversions between Python's `datetime.datetime` and [`jiff::civil::DateTime`]

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDateAccess, PyDateTime, PyTimeAccess, PyTzInfoAccess};

/// Converts a naive Python `datetime` into a [`jiff::civil::DateTime`].
///
/// Timezone aware datetimes are rejected.
pub fn py_datetime_to_civil(datetime: &Bound<'_, PyDateTime>) -> PyResult<jiff::civil::DateTime> {
    if datetime.get_tzinfo().is_some() {
        return Err(PyTypeError::new_err("expected a datetime without tzinfo"));
    }

    let year = i16::try_from(datetime.get_year())
        .map_err(|_| PyValueError::new_err("invalid or out-of-range date"))?;

    jiff::civil::DateTime::new(
        year,
        datetime.get_month() as i8,
        datetime.get_day() as i8,
        datetime.get_hour() as i8,
        datetime.get_minute() as i8,
        datetime.get_second() as i8,
        datetime.get_microsecond() as i32 * 1000,
    )
    .map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Converts a [`jiff::civil::DateTime`] into a naive Python `datetime`.
///
/// Sub-microsecond precision is truncated.
pub fn civil_to_py_datetime(
    py: Python<'_>,
    datetime: jiff::civil::DateTime,
) -> PyResult<Bound<'_, PyDateTime>> {
    let year = datetime.year();
    if year < 1 {
        return Err(PyValueError::new_err("invalid or out-of-range date"));
    }

    PyDateTime::new(
        py,
        year.into(),
        datetime.month() as u8,
        datetime.day() as u8,
        datetime.hour() as u8,
        datetime.minute() as u8,
        datetime.second() as u8,
        (datetime.subsec_nanosecond() / 1000) as u32,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil;
    use std::ffi::CStr;

    fn eval<'py>(py: Python<'py>, expr: &CStr) -> Bound<'py, PyDateTime> {
        let globals = pyo3::types::PyDict::new(py);
        py.run(c"import datetime", Some(&globals), None).unwrap();
        py.eval(expr, Some(&globals), None)
            .unwrap()
            .cast_into()
            .unwrap()
    }

    #[test]
    fn python_to_civil() {
        Python::initialize();
        Python::attach(|py| {
            let datetime = eval(py, c"datetime.datetime(2024, 2, 29, 23, 59, 58, 123456)");
            assert_eq!(
                py_datetime_to_civil(&datetime).unwrap(),
                civil::datetime(2024, 2, 29, 23, 59, 58, 123_456_000)
            );
        });
    }

    #[test]
    fn python_with_tzinfo_is_rejected() {
        Python::initialize();
        Python::attach(|py| {
            let datetime = eval(
                py,
                c"datetime.datetime(2024, 1, 1, tzinfo=datetime.timezone.utc)",
            );
            let err = py_datetime_to_civil(&datetime).unwrap_err();
            assert!(err.is_instance_of::<PyTypeError>(py));
        });
    }

    #[test]
    fn civil_to_python() {
        Python::initialize();
        Python::attach(|py| {
            let datetime =
                civil_to_py_datetime(py, civil::datetime(2026, 10, 5, 13, 37, 42, 123_456_789))
                    .unwrap();
            let expected = eval(py, c"datetime.datetime(2026, 10, 5, 13, 37, 42, 123456)");
            assert!(datetime.eq(expected).unwrap());
            assert!(datetime.get_tzinfo().is_none());
        });
    }

    #[test]
    fn civil_before_year_1_is_rejected() {
        Python::initialize();
        Python::attach(|py| {
            let err = civil_to_py_datetime(py, civil::datetime(0, 1, 1, 0, 0, 0, 0)).unwrap_err();
            assert!(err.is_instance_of::<PyValueError>(py));
        });
    }
}
