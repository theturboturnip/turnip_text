use std::marker::PhantomData;

use pyo3::{exceptions::PyTypeError, intern, prelude::*, types::PyList, PyClass};

pub trait PyTypeclass {
    const NAME: &'static str;
    fn fits_typeclass(obj: &Bound<'_, PyAny>) -> PyResult<bool>;
}

#[derive(Debug, Clone)]
pub struct PyInstanceTypeclass<T: PyClass>(PhantomData<T>);
impl<T: PyClass> PyTypeclass for PyInstanceTypeclass<T> {
    const NAME: &'static str = T::NAME;

    fn fits_typeclass(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(obj.is_instance_of::<T>())
    }
}

#[derive(Debug, Clone)]
pub struct PyTcRef<T: PyTypeclass>(PyObject, PhantomData<T>);
impl<T: PyTypeclass> PyTcRef<T> {
    pub fn of(obj: &Bound<'_, PyAny>) -> Result<Self, ()> {
        match T::fits_typeclass(obj) {
            Ok(true) => Ok(Self(obj.to_object(obj.py()), PhantomData::default())),
            Ok(false) | Err(_) => Err(()),
        }
    }

    pub fn of_friendly(obj: &Bound<'_, PyAny>, context: &str) -> PyResult<Self> {
        if T::fits_typeclass(obj)? {
            Ok(Self(obj.to_object(obj.py()), PhantomData::default()))
        } else {
            let obj_repr = obj.repr()?;
            // TODO make fits_typeclass() return a specific TypeError instead of doing a catch-all here.
            Err(PyTypeError::new_err(format!(
                "Expected {} to be an instance of {}, but it wasn't. Got {}",
                context,
                T::NAME,
                obj_repr.to_str()?
            )))
        }
    }

    pub fn of_unchecked(obj: &Bound<'_, PyAny>) -> Self {
        Self(obj.to_object(obj.py()), PhantomData::default())
    }

    pub fn bind<'py>(&self, py: Python<'py>) -> &Bound<'py, PyAny> {
        self.0.bind(py)
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
        Self(PyList::empty_bound(py).into(), PhantomData::default())
    }

    /// Given a pre-existing Python list, pass in
    pub fn from(list: &Bound<'_, PyList>) -> PyResult<Self> {
        for obj in list {
            if !T::fits_typeclass(&obj)? {
                let obj_repr = obj.repr()?;
                return Err(PyTypeError::new_err(format!(
                    "Passed list containing object {} into PyTypeclassList constructor -- \
                     expected object fitting typeclass {}, didn't get it",
                    obj_repr.to_str()?,
                    T::NAME
                )));
            }
        }
        Ok(Self(
            list.as_unbound().clone_ref(list.py()),
            PhantomData::default(),
        ))
    }

    pub fn append_checked(&self, val: &Bound<'_, PyAny>) -> PyResult<()> {
        if T::fits_typeclass(val)? {
            self.0.bind(val.py()).append(val)?;
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

    pub fn list<'py>(&self, py: Python<'py>) -> &Bound<'py, PyList> {
        self.0.bind(py)
    }

    pub fn __eq__(&self, py: Python, other: &Self) -> PyResult<bool> {
        self.0
            .bind(py)
            .getattr(intern!(py, "__eq__"))?
            .call1((other.0.bind(py),))?
            .is_truthy()
    }
    pub fn __repr__(&self, py: Python) -> PyResult<String> {
        Ok(format!(
            "PyTypeclassList<{}>({})",
            T::NAME,
            self.0.bind(py).str()?.to_str()?
        ))
    }
}

/// [PyTypeclassList] equivalent for objects which subclass the given type T
pub type PyInstanceList<T> = PyTypeclassList<PyInstanceTypeclass<T>>;
