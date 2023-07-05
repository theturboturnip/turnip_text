use std::marker::PhantomData;

use pyo3::{exceptions::PyTypeError, prelude::*, types::PyList, PyClass};

pub trait PyTypeclass {
    const NAME: &'static str;
    fn fits_typeclass(obj: &PyAny) -> PyResult<bool>;
}

#[derive(Debug, Clone)]
pub struct PyInstanceTypeclass<T: PyClass>(PhantomData<T>);
impl<T: PyClass> PyTypeclass for PyInstanceTypeclass<T> {
    const NAME: &'static str = T::NAME;

    fn fits_typeclass(obj: &PyAny) -> PyResult<bool> {
        obj.is_instance_of::<T>()
    }
}

#[derive(Debug, Clone)]
pub struct PyTcRef<T: PyTypeclass>(PyObject, PhantomData<T>);
impl<T: PyTypeclass> PyTcRef<T> {
    pub fn of(val: &PyAny) -> PyResult<Self> {
        if T::fits_typeclass(val)? {
            Ok(Self(val.into(), PhantomData::default()))
        } else {
            let obj_repr = val.repr()?;
            Err(PyTypeError::new_err(format!(
                "Expected object fitting typeclass {}, didn't get it. Got {}",
                T::NAME,
                obj_repr.to_str()?
            )))
        }
    }

    pub fn as_ref<'py>(&'py self, py: Python<'py>) -> &'py PyAny {
        self.0.as_ref(py)
    }

    pub fn unbox(self) -> PyObject {
        self.0
    }
}

/// Wrapper for [PyList] which provides the [append_checked] function,
/// ensuring all items appended fit the provided [PyTypeclass].
#[derive(Debug, Clone)]
pub struct PyTypeclassList<T: PyTypeclass>(Py<PyList>, PhantomData<T>);
impl<T: PyTypeclass> PyTypeclassList<T> {
    pub fn new(py: Python) -> Self {
        Self(PyList::empty(py).into(), PhantomData::default())
    }

    /// Given a pre-existing Python list, pass in
    pub fn from(list: &PyList) -> PyResult<Self> {
        for obj in list {
            if !T::fits_typeclass(obj)? {
                let obj_repr = obj.repr()?;
                return Err(PyTypeError::new_err(format!(
                    "Passed list containing object {} into PyTypeclassList constructor -- expected object fitting typeclass {}, didn't get it",
                    obj_repr.to_str()?,
                    T::NAME
                )));
            }
        }
        Ok(Self(list.into(), PhantomData::default()))
    }

    pub fn append_checked(&self, val: &PyAny) -> PyResult<()> {
        if T::fits_typeclass(val)? {
            self.0.as_ref(val.py()).append(val)?;
            Ok(())
        } else {
            let obj_repr = val.repr()?;
            Err(PyTypeError::new_err(format!(
                "Expected object fitting typeclass {}, didn't get it. Got {}",
                T::NAME,
                obj_repr.to_str()?
            )))
        }
    }

    pub fn list<'py>(&'py self, py: Python<'py>) -> &'py PyList {
        self.0.as_ref(py)
    }
}

/// [PyTypeclassList] equivalent for objects which subclass the given type T
pub type PyInstanceList<T> = PyTypeclassList<PyInstanceTypeclass<T>>;
