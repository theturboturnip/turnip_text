use std::marker::PhantomData;

use pyo3::{
    exceptions::PyTypeError,
    intern,
    prelude::*,
    types::{PyIterator, PyList},
    PyClass,
};

pub trait PyTypeclass {
    const NAME: &'static str;
    /// Return true if the given object fits the typeclass
    fn fits_typeclass(obj: &Bound<'_, PyAny>) -> PyResult<bool>;
    /// Return Some(err) if the given object *doesn't* fit the typeclass, where the error provides a helpful message
    fn get_typeclass_err(obj: &Bound<'_, PyAny>, context: &str) -> PyResult<Option<PyErr>>;
}

#[derive(Debug, Clone)]
pub struct PyInstanceTypeclass<T: PyClass>(PhantomData<T>);
impl<T: PyClass> PyTypeclass for PyInstanceTypeclass<T> {
    const NAME: &'static str = T::NAME;

    fn fits_typeclass(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(obj.is_instance_of::<T>())
    }
    fn get_typeclass_err(obj: &Bound<'_, PyAny>, context: &str) -> PyResult<Option<PyErr>> {
        if obj.is_instance_of::<T>() {
            Ok(None)
        } else {
            let obj_repr = obj.repr()?;
            let err = PyTypeError::new_err(format!(
                "Expected {} to be an instance of {}, but it wasn't. Got {}",
                context,
                T::NAME,
                obj_repr.to_str()?
            ));
            Ok(Some(err))
        }
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
        match T::get_typeclass_err(obj, context)? {
            None => {
                // It fits the typeclass
                Ok(Self(obj.to_object(obj.py()), PhantomData::default()))
            }
            Some(err) => Err(err),
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

    /// Given a Python iterable, append_checked each element into a list
    pub fn wrap_iter(iter: &Bound<'_, PyIterator>) -> PyResult<Self> {
        let list = Self::new(iter.py());
        for obj in iter {
            list.append_checked(&obj?)?;
        }
        Ok(list)
    }

    /// Given a pre-existing Python list, check all elements
    pub fn wrap_list(list: &Bound<'_, PyList>) -> PyResult<Self> {
        for obj in list {
            if let Some(err) = T::get_typeclass_err(&obj, "list element")? {
                return Err(err);
            }
        }
        Ok(Self(
            list.as_unbound().clone_ref(list.py()),
            PhantomData::default(),
        ))
    }

    pub fn append_checked(&self, obj: &Bound<'_, PyAny>) -> PyResult<()> {
        match T::get_typeclass_err(obj, "new list element")? {
            None => {
                // It fits the typeclass
                self.0.bind(obj.py()).append(obj)?;
                Ok(())
            }
            Some(err) => Err(err),
        }
    }

    pub fn insert_checked(&self, index: usize, obj: &Bound<'_, PyAny>) -> PyResult<()> {
        match T::get_typeclass_err(obj, "new list element")? {
            None => {
                // It fits the typeclass
                self.0.bind(obj.py()).insert(index, obj)?;
                Ok(())
            }
            Some(err) => Err(err),
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
