/*! Python `property` descriptor class.

*/
use super::{PyStrRef, PyType, PyTypeRef};
use crate::common::lock::PyRwLock;
use crate::function::{IntoFuncArgs, PosArgs};
use crate::{
    AsObject, Context, Py, PyObject, PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine,
    class::PyClassImpl,
    function::{FuncArgs, PySetterValue},
    types::{Constructor, GetDescriptor, Initializer},
};

#[pyclass(module = false, name = "property", traverse)]
#[derive(Debug)]
pub struct PyProperty {
    getter: PyRwLock<Option<PyObjectRef>>,
    setter: PyRwLock<Option<PyObjectRef>>,
    deleter: PyRwLock<Option<PyObjectRef>>,
    doc: PyRwLock<Option<PyObjectRef>>,
    name: PyRwLock<Option<PyObjectRef>>,
}

impl PyPayload for PyProperty {
    fn class(ctx: &Context) -> &'static Py<PyType> {
        ctx.types.property_type
    }
}

#[derive(FromArgs)]
pub struct PropertyArgs {
    #[pyarg(any, default)]
    fget: Option<PyObjectRef>,
    #[pyarg(any, default)]
    fset: Option<PyObjectRef>,
    #[pyarg(any, default)]
    fdel: Option<PyObjectRef>,
    #[pyarg(any, default)]
    doc: Option<PyObjectRef>,
    #[pyarg(any, default)]
    name: Option<PyStrRef>,
}

impl GetDescriptor for PyProperty {
    fn descr_get(
        zelf_obj: PyObjectRef,
        obj: Option<PyObjectRef>,
        _cls: Option<PyObjectRef>,
        vm: &VirtualMachine,
    ) -> PyResult {
        let (zelf, obj) = Self::_unwrap(&zelf_obj, obj, vm)?;
        if vm.is_none(&obj) {
            Ok(zelf_obj)
        } else if let Some(getter) = zelf.getter.read().as_ref() {
            getter.call((obj,), vm)
        } else {
            Err(vm.new_attribute_error("property has no getter".to_string()))
        }
    }
}

#[pyclass(with(Constructor, Initializer, GetDescriptor), flags(BASETYPE))]
impl PyProperty {
    // Descriptor methods

    #[pyslot]
    fn descr_set(
        zelf: &PyObject,
        obj: PyObjectRef,
        value: PySetterValue,
        vm: &VirtualMachine,
    ) -> PyResult<()> {
        let zelf = zelf.try_to_ref::<Self>(vm)?;
        match value {
            PySetterValue::Assign(value) => {
                if let Some(setter) = zelf.setter.read().as_ref() {
                    setter.call((obj, value), vm).map(drop)
                } else {
                    Err(vm.new_attribute_error("property has no setter".to_owned()))
                }
            }
            PySetterValue::Delete => {
                if let Some(deleter) = zelf.deleter.read().as_ref() {
                    deleter.call((obj,), vm).map(drop)
                } else {
                    Err(vm.new_attribute_error("property has no deleter".to_owned()))
                }
            }
        }
    }
    #[pymethod]
    fn __set__(
        zelf: PyObjectRef,
        obj: PyObjectRef,
        value: PyObjectRef,
        vm: &VirtualMachine,
    ) -> PyResult<()> {
        Self::descr_set(&zelf, obj, PySetterValue::Assign(value), vm)
    }
    #[pymethod]
    fn __delete__(zelf: PyObjectRef, obj: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
        Self::descr_set(&zelf, obj, PySetterValue::Delete, vm)
    }

    // Access functions

    #[pygetset]
    fn fget(&self) -> Option<PyObjectRef> {
        self.getter.read().clone()
    }

    #[pygetset]
    fn fset(&self) -> Option<PyObjectRef> {
        self.setter.read().clone()
    }

    #[pygetset]
    fn fdel(&self) -> Option<PyObjectRef> {
        self.deleter.read().clone()
    }

    fn doc_getter(&self) -> Option<PyObjectRef> {
        self.doc.read().clone()
    }
    fn doc_setter(&self, value: Option<PyObjectRef>) {
        *self.doc.write() = value;
    }

    #[pymethod(magic)]
    fn set_name(&self, args: PosArgs, vm: &VirtualMachine) -> PyResult<()> {
        let func_args = args.into_args(vm);
        let func_args_len = func_args.args.len();
        let (_owner, name): (PyObjectRef, PyObjectRef) = func_args.bind(vm).map_err(|_e| {
            vm.new_type_error(format!(
                "__set_name__() takes 2 positional arguments but {func_args_len} were given"
            ))
        })?;

        *self.name.write() = Some(name);

        Ok(())
    }

    // Python builder functions

    #[pymethod]
    fn getter(
        zelf: PyRef<Self>,
        getter: Option<PyObjectRef>,
        vm: &VirtualMachine,
    ) -> PyResult<PyRef<Self>> {
        PyProperty {
            getter: PyRwLock::new(getter.or_else(|| zelf.fget())),
            setter: PyRwLock::new(zelf.fset()),
            deleter: PyRwLock::new(zelf.fdel()),
            doc: PyRwLock::new(None),
            name: PyRwLock::new(None),
        }
        .into_ref_with_type(vm, zelf.class().to_owned())
    }

    #[pymethod]
    fn setter(
        zelf: PyRef<Self>,
        setter: Option<PyObjectRef>,
        vm: &VirtualMachine,
    ) -> PyResult<PyRef<Self>> {
        PyProperty {
            getter: PyRwLock::new(zelf.fget()),
            setter: PyRwLock::new(setter.or_else(|| zelf.fset())),
            deleter: PyRwLock::new(zelf.fdel()),
            doc: PyRwLock::new(None),
            name: PyRwLock::new(None),
        }
        .into_ref_with_type(vm, zelf.class().to_owned())
    }

    #[pymethod]
    fn deleter(
        zelf: PyRef<Self>,
        deleter: Option<PyObjectRef>,
        vm: &VirtualMachine,
    ) -> PyResult<PyRef<Self>> {
        PyProperty {
            getter: PyRwLock::new(zelf.fget()),
            setter: PyRwLock::new(zelf.fset()),
            deleter: PyRwLock::new(deleter.or_else(|| zelf.fdel())),
            doc: PyRwLock::new(None),
            name: PyRwLock::new(None),
        }
        .into_ref_with_type(vm, zelf.class().to_owned())
    }

    #[pygetset(magic)]
    fn isabstractmethod(&self, vm: &VirtualMachine) -> PyObjectRef {
        let getter_abstract = match self.getter.read().to_owned() {
            Some(getter) => getter
                .get_attr("__isabstractmethod__", vm)
                .unwrap_or_else(|_| vm.ctx.new_bool(false).into()),
            _ => vm.ctx.new_bool(false).into(),
        };
        let setter_abstract = match self.setter.read().to_owned() {
            Some(setter) => setter
                .get_attr("__isabstractmethod__", vm)
                .unwrap_or_else(|_| vm.ctx.new_bool(false).into()),
            _ => vm.ctx.new_bool(false).into(),
        };
        vm._or(&setter_abstract, &getter_abstract)
            .unwrap_or_else(|_| vm.ctx.new_bool(false).into())
    }

    #[pygetset(magic, setter)]
    fn set_isabstractmethod(&self, value: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
        if let Some(getter) = self.getter.read().to_owned() {
            getter.set_attr("__isabstractmethod__", value, vm)?;
        }
        Ok(())
    }
}

impl Constructor for PyProperty {
    type Args = FuncArgs;

    fn py_new(cls: PyTypeRef, _args: FuncArgs, vm: &VirtualMachine) -> PyResult {
        PyProperty {
            getter: PyRwLock::new(None),
            setter: PyRwLock::new(None),
            deleter: PyRwLock::new(None),
            doc: PyRwLock::new(None),
            name: PyRwLock::new(None),
        }
        .into_ref_with_type(vm, cls)
        .map(Into::into)
    }
}

impl Initializer for PyProperty {
    type Args = PropertyArgs;

    fn init(zelf: PyRef<Self>, args: Self::Args, _vm: &VirtualMachine) -> PyResult<()> {
        *zelf.getter.write() = args.fget;
        *zelf.setter.write() = args.fset;
        *zelf.deleter.write() = args.fdel;
        *zelf.doc.write() = args.doc;
        *zelf.name.write() = args.name.map(|a| a.as_object().to_owned());

        Ok(())
    }
}

pub(crate) fn init(context: &Context) {
    PyProperty::extend_class(context, context.types.property_type);

    // This is a bit unfortunate, but this instance attribute overlaps with the
    // class __doc__ string..
    extend_class!(context, context.types.property_type, {
        "__doc__" => context.new_getset(
            "__doc__",
            context.types.property_type,
            PyProperty::doc_getter,
            PyProperty::doc_setter,
        ),
    });
}
