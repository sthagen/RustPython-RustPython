pub(crate) use _collections::make_module;

#[pymodule]
mod _collections {
    use crate::{
        AsObject, Py, PyObject, PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine,
        atomic_func,
        builtins::{
            IterStatus::{Active, Exhausted},
            PositionIterInternal, PyGenericAlias, PyInt, PyTypeRef,
        },
        common::lock::{PyMutex, PyRwLock, PyRwLockReadGuard, PyRwLockWriteGuard},
        function::{KwArgs, OptionalArg, PyComparisonValue},
        iter::PyExactSizeIterator,
        protocol::{PyIterReturn, PySequenceMethods},
        recursion::ReprGuard,
        sequence::{MutObjectSequenceOp, OptionalRangeArgs},
        sliceable::SequenceIndexOp,
        types::{
            AsSequence, Comparable, Constructor, DefaultConstructor, Initializer, IterNext,
            Iterable, PyComparisonOp, Representable, SelfIter,
        },
        utils::collection_repr,
    };
    use crossbeam_utils::atomic::AtomicCell;
    use std::cmp::max;
    use std::collections::VecDeque;

    #[pyattr]
    #[pyclass(module = "collections", name = "deque", unhashable = true)]
    #[derive(Debug, Default, PyPayload)]
    struct PyDeque {
        deque: PyRwLock<VecDeque<PyObjectRef>>,
        maxlen: Option<usize>,
        state: AtomicCell<usize>, // incremented whenever the indices move
    }

    type PyDequeRef = PyRef<PyDeque>;

    #[derive(FromArgs)]
    struct PyDequeOptions {
        #[pyarg(any, optional)]
        iterable: OptionalArg<PyObjectRef>,
        #[pyarg(any, optional)]
        maxlen: OptionalArg<PyObjectRef>,
    }

    impl PyDeque {
        fn borrow_deque(&self) -> PyRwLockReadGuard<'_, VecDeque<PyObjectRef>> {
            self.deque.read()
        }

        fn borrow_deque_mut(&self) -> PyRwLockWriteGuard<'_, VecDeque<PyObjectRef>> {
            self.deque.write()
        }
    }

    #[pyclass(
        flags(BASETYPE),
        with(
            Constructor,
            Initializer,
            AsSequence,
            Comparable,
            Iterable,
            Representable
        )
    )]
    impl PyDeque {
        #[pymethod]
        fn append(&self, obj: PyObjectRef) {
            self.state.fetch_add(1);
            let mut deque = self.borrow_deque_mut();
            if self.maxlen == Some(deque.len()) {
                deque.pop_front();
            }
            deque.push_back(obj);
        }

        #[pymethod]
        fn appendleft(&self, obj: PyObjectRef) {
            self.state.fetch_add(1);
            let mut deque = self.borrow_deque_mut();
            if self.maxlen == Some(deque.len()) {
                deque.pop_back();
            }
            deque.push_front(obj);
        }

        #[pymethod]
        fn clear(&self) {
            self.state.fetch_add(1);
            self.borrow_deque_mut().clear()
        }

        #[pymethod(name = "__copy__")]
        #[pymethod]
        fn copy(zelf: PyRef<Self>, vm: &VirtualMachine) -> PyResult<PyRef<Self>> {
            Self {
                deque: PyRwLock::new(zelf.borrow_deque().clone()),
                maxlen: zelf.maxlen,
                state: AtomicCell::new(zelf.state.load()),
            }
            .into_ref_with_type(vm, zelf.class().to_owned())
        }

        #[pymethod]
        fn count(&self, obj: PyObjectRef, vm: &VirtualMachine) -> PyResult<usize> {
            let start_state = self.state.load();
            let count = self.mut_count(vm, &obj)?;

            if start_state != self.state.load() {
                return Err(vm.new_runtime_error("deque mutated during iteration"));
            }
            Ok(count)
        }

        #[pymethod]
        fn extend(&self, iter: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
            self._extend(&iter, vm)
        }

        fn _extend(&self, iter: &PyObject, vm: &VirtualMachine) -> PyResult<()> {
            self.state.fetch_add(1);
            let max_len = self.maxlen;
            let mut elements: Vec<PyObjectRef> = iter.try_to_value(vm)?;
            if let Some(max_len) = max_len {
                if max_len > elements.len() {
                    let mut deque = self.borrow_deque_mut();
                    let drain_until = deque.len().saturating_sub(max_len - elements.len());
                    deque.drain(..drain_until);
                } else {
                    self.borrow_deque_mut().clear();
                    elements.drain(..(elements.len() - max_len));
                }
            }
            self.borrow_deque_mut().extend(elements);
            Ok(())
        }

        #[pymethod]
        fn extendleft(&self, iter: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
            let max_len = self.maxlen;
            let mut elements: Vec<PyObjectRef> = iter.try_to_value(vm)?;
            elements.reverse();

            if let Some(max_len) = max_len {
                if max_len > elements.len() {
                    let mut deque = self.borrow_deque_mut();
                    let truncate_until = max_len - elements.len();
                    deque.truncate(truncate_until);
                } else {
                    self.borrow_deque_mut().clear();
                    elements.truncate(max_len);
                }
            }
            let mut created = VecDeque::from(elements);
            let mut borrowed = self.borrow_deque_mut();
            created.append(&mut borrowed);
            std::mem::swap(&mut created, &mut borrowed);
            Ok(())
        }

        #[pymethod]
        fn index(
            &self,
            needle: PyObjectRef,
            range: OptionalRangeArgs,
            vm: &VirtualMachine,
        ) -> PyResult<usize> {
            let start_state = self.state.load();

            let (start, stop) = range.saturate(self.__len__(), vm)?;
            let index = self.mut_index_range(vm, &needle, start..stop)?;
            if start_state != self.state.load() {
                Err(vm.new_runtime_error("deque mutated during iteration"))
            } else if let Some(index) = index.into() {
                Ok(index)
            } else {
                Err(vm.new_value_error(
                    needle
                        .repr(vm)
                        .map(|repr| format!("{repr} is not in deque"))
                        .unwrap_or_else(|_| String::new()),
                ))
            }
        }

        #[pymethod]
        fn insert(&self, idx: i32, obj: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
            self.state.fetch_add(1);
            let mut deque = self.borrow_deque_mut();

            if self.maxlen == Some(deque.len()) {
                return Err(vm.new_index_error("deque already at its maximum size"));
            }

            let idx = if idx < 0 {
                if -idx as usize > deque.len() {
                    0
                } else {
                    deque.len() - ((-idx) as usize)
                }
            } else if idx as usize > deque.len() {
                deque.len()
            } else {
                idx as usize
            };

            deque.insert(idx, obj);

            Ok(())
        }

        #[pymethod]
        fn pop(&self, vm: &VirtualMachine) -> PyResult {
            self.state.fetch_add(1);
            self.borrow_deque_mut()
                .pop_back()
                .ok_or_else(|| vm.new_index_error("pop from an empty deque"))
        }

        #[pymethod]
        fn popleft(&self, vm: &VirtualMachine) -> PyResult {
            self.state.fetch_add(1);
            self.borrow_deque_mut()
                .pop_front()
                .ok_or_else(|| vm.new_index_error("pop from an empty deque"))
        }

        #[pymethod]
        fn remove(&self, obj: PyObjectRef, vm: &VirtualMachine) -> PyResult {
            let start_state = self.state.load();
            let index = self.mut_index(vm, &obj)?;

            if start_state != self.state.load() {
                Err(vm.new_index_error("deque mutated during remove()."))
            } else if let Some(index) = index.into() {
                let mut deque = self.borrow_deque_mut();
                self.state.fetch_add(1);
                Ok(deque.remove(index).unwrap())
            } else {
                Err(vm.new_value_error("deque.remove(x): x not in deque"))
            }
        }

        #[pymethod]
        fn reverse(&self) {
            let rev: VecDeque<_> = self.borrow_deque().iter().cloned().rev().collect();
            *self.borrow_deque_mut() = rev;
        }

        #[pymethod]
        fn __reversed__(zelf: PyRef<Self>) -> PyResult<PyReverseDequeIterator> {
            Ok(PyReverseDequeIterator {
                state: zelf.state.load(),
                internal: PyMutex::new(PositionIterInternal::new(zelf, 0)),
            })
        }

        #[pymethod]
        fn rotate(&self, mid: OptionalArg<isize>) {
            self.state.fetch_add(1);
            let mut deque = self.borrow_deque_mut();
            if !deque.is_empty() {
                let mid = mid.unwrap_or(1) % deque.len() as isize;
                if mid.is_negative() {
                    deque.rotate_left(-mid as usize);
                } else {
                    deque.rotate_right(mid as usize);
                }
            }
        }

        #[pygetset]
        const fn maxlen(&self) -> Option<usize> {
            self.maxlen
        }

        #[pymethod]
        fn __getitem__(&self, idx: isize, vm: &VirtualMachine) -> PyResult {
            let deque = self.borrow_deque();
            idx.wrapped_at(deque.len())
                .and_then(|i| deque.get(i).cloned())
                .ok_or_else(|| vm.new_index_error("deque index out of range"))
        }

        #[pymethod]
        fn __setitem__(&self, idx: isize, value: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
            let mut deque = self.borrow_deque_mut();
            idx.wrapped_at(deque.len())
                .and_then(|i| deque.get_mut(i))
                .map(|x| *x = value)
                .ok_or_else(|| vm.new_index_error("deque index out of range"))
        }

        #[pymethod]
        fn __delitem__(&self, idx: isize, vm: &VirtualMachine) -> PyResult<()> {
            let mut deque = self.borrow_deque_mut();
            idx.wrapped_at(deque.len())
                .and_then(|i| deque.remove(i).map(drop))
                .ok_or_else(|| vm.new_index_error("deque index out of range"))
        }

        #[pymethod]
        fn __contains__(&self, needle: PyObjectRef, vm: &VirtualMachine) -> PyResult<bool> {
            self._contains(&needle, vm)
        }

        fn _contains(&self, needle: &PyObject, vm: &VirtualMachine) -> PyResult<bool> {
            let start_state = self.state.load();
            let ret = self.mut_contains(vm, needle)?;
            if start_state != self.state.load() {
                Err(vm.new_runtime_error("deque mutated during iteration"))
            } else {
                Ok(ret)
            }
        }

        fn _mul(&self, n: isize, vm: &VirtualMachine) -> PyResult<VecDeque<PyObjectRef>> {
            let deque = self.borrow_deque();
            let n = vm.check_repeat_or_overflow_error(deque.len(), n)?;
            let mul_len = n * deque.len();
            let iter = deque.iter().cycle().take(mul_len);
            let skipped = self
                .maxlen
                .and_then(|maxlen| mul_len.checked_sub(maxlen))
                .unwrap_or(0);

            let deque = iter.skip(skipped).cloned().collect();
            Ok(deque)
        }

        #[pymethod]
        #[pymethod(name = "__rmul__")]
        fn __mul__(&self, n: isize, vm: &VirtualMachine) -> PyResult<Self> {
            let deque = self._mul(n, vm)?;
            Ok(Self {
                deque: PyRwLock::new(deque),
                maxlen: self.maxlen,
                state: AtomicCell::new(0),
            })
        }

        #[pymethod]
        fn __imul__(zelf: PyRef<Self>, n: isize, vm: &VirtualMachine) -> PyResult<PyRef<Self>> {
            let mul_deque = zelf._mul(n, vm)?;
            *zelf.borrow_deque_mut() = mul_deque;
            Ok(zelf)
        }

        #[pymethod]
        fn __len__(&self) -> usize {
            self.borrow_deque().len()
        }

        #[pymethod]
        fn __bool__(&self) -> bool {
            !self.borrow_deque().is_empty()
        }

        #[pymethod]
        fn __add__(&self, other: PyObjectRef, vm: &VirtualMachine) -> PyResult<Self> {
            self.concat(&other, vm)
        }

        fn concat(&self, other: &PyObject, vm: &VirtualMachine) -> PyResult<Self> {
            if let Some(o) = other.downcast_ref::<Self>() {
                let mut deque = self.borrow_deque().clone();
                let elements = o.borrow_deque().clone();
                deque.extend(elements);

                let skipped = self
                    .maxlen
                    .and_then(|maxlen| deque.len().checked_sub(maxlen))
                    .unwrap_or(0);
                deque.drain(..skipped);

                Ok(Self {
                    deque: PyRwLock::new(deque),
                    maxlen: self.maxlen,
                    state: AtomicCell::new(0),
                })
            } else {
                Err(vm.new_type_error(format!(
                    "can only concatenate deque (not \"{}\") to deque",
                    other.class().name()
                )))
            }
        }

        #[pymethod]
        fn __iadd__(
            zelf: PyRef<Self>,
            other: PyObjectRef,
            vm: &VirtualMachine,
        ) -> PyResult<PyRef<Self>> {
            zelf.extend(other, vm)?;
            Ok(zelf)
        }

        #[pymethod]
        fn __reduce__(zelf: PyRef<Self>, vm: &VirtualMachine) -> PyResult {
            let cls = zelf.class().to_owned();
            let value = match zelf.maxlen {
                Some(v) => vm.new_pyobj((vm.ctx.empty_tuple.clone(), v)),
                None => vm.ctx.empty_tuple.clone().into(),
            };
            Ok(vm.new_pyobj((cls, value, vm.ctx.none(), PyDequeIterator::new(zelf))))
        }

        #[pyclassmethod]
        fn __class_getitem__(
            cls: PyTypeRef,
            args: PyObjectRef,
            vm: &VirtualMachine,
        ) -> PyGenericAlias {
            PyGenericAlias::from_args(cls, args, vm)
        }
    }

    impl MutObjectSequenceOp for PyDeque {
        type Inner = VecDeque<PyObjectRef>;

        fn do_get(index: usize, inner: &Self::Inner) -> Option<&PyObjectRef> {
            inner.get(index)
        }

        fn do_lock(&self) -> impl std::ops::Deref<Target = Self::Inner> {
            self.borrow_deque()
        }
    }

    impl DefaultConstructor for PyDeque {}

    impl Initializer for PyDeque {
        type Args = PyDequeOptions;

        fn init(
            zelf: PyRef<Self>,
            PyDequeOptions { iterable, maxlen }: Self::Args,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            // TODO: This is _basically_ pyobject_to_opt_usize in itertools.rs
            // need to move that function elsewhere and refactor usages.
            let maxlen = if let Some(obj) = maxlen.into_option() {
                if !vm.is_none(&obj) {
                    let maxlen: isize = obj
                        .downcast_ref::<PyInt>()
                        .ok_or_else(|| vm.new_type_error("an integer is required."))?
                        .try_to_primitive(vm)?;

                    if maxlen.is_negative() {
                        return Err(vm.new_value_error("maxlen must be non-negative."));
                    }
                    Some(maxlen as usize)
                } else {
                    None
                }
            } else {
                None
            };

            // retrieve elements first to not to make too huge lock
            let elements = iterable
                .into_option()
                .map(|iter| {
                    let mut elements: Vec<PyObjectRef> = iter.try_to_value(vm)?;
                    if let Some(maxlen) = maxlen {
                        elements.drain(..elements.len().saturating_sub(maxlen));
                    }
                    Ok(elements)
                })
                .transpose()?;

            // SAFETY: This is hacky part for read-only field
            // Because `maxlen` is only mutated from __init__. We can abuse the lock of deque to ensure this is locked enough.
            // If we make a single lock of deque not only for extend but also for setting maxlen, it will be safe.
            {
                let mut deque = zelf.borrow_deque_mut();
                // Clear any previous data present.
                deque.clear();
                unsafe {
                    // `maxlen` is better to be defined as UnsafeCell in common practice,
                    // but then more type works without any safety benefits
                    let unsafe_maxlen =
                        &zelf.maxlen as *const _ as *const std::cell::UnsafeCell<Option<usize>>;
                    *(*unsafe_maxlen).get() = maxlen;
                }
                if let Some(elements) = elements {
                    deque.extend(elements);
                }
            }

            Ok(())
        }
    }

    impl AsSequence for PyDeque {
        fn as_sequence() -> &'static PySequenceMethods {
            static AS_SEQUENCE: PySequenceMethods = PySequenceMethods {
                length: atomic_func!(|seq, _vm| Ok(PyDeque::sequence_downcast(seq).__len__())),
                concat: atomic_func!(|seq, other, vm| {
                    PyDeque::sequence_downcast(seq)
                        .concat(other, vm)
                        .map(|x| x.into_ref(&vm.ctx).into())
                }),
                repeat: atomic_func!(|seq, n, vm| {
                    PyDeque::sequence_downcast(seq)
                        .__mul__(n, vm)
                        .map(|x| x.into_ref(&vm.ctx).into())
                }),
                item: atomic_func!(|seq, i, vm| PyDeque::sequence_downcast(seq).__getitem__(i, vm)),
                ass_item: atomic_func!(|seq, i, value, vm| {
                    let zelf = PyDeque::sequence_downcast(seq);
                    if let Some(value) = value {
                        zelf.__setitem__(i, value, vm)
                    } else {
                        zelf.__delitem__(i, vm)
                    }
                }),
                contains: atomic_func!(
                    |seq, needle, vm| PyDeque::sequence_downcast(seq)._contains(needle, vm)
                ),
                inplace_concat: atomic_func!(|seq, other, vm| {
                    let zelf = PyDeque::sequence_downcast(seq);
                    zelf._extend(other, vm)?;
                    Ok(zelf.to_owned().into())
                }),
                inplace_repeat: atomic_func!(|seq, n, vm| {
                    let zelf = PyDeque::sequence_downcast(seq);
                    PyDeque::__imul__(zelf.to_owned(), n, vm).map(|x| x.into())
                }),
            };

            &AS_SEQUENCE
        }
    }

    impl Comparable for PyDeque {
        fn cmp(
            zelf: &Py<Self>,
            other: &PyObject,
            op: PyComparisonOp,
            vm: &VirtualMachine,
        ) -> PyResult<PyComparisonValue> {
            if let Some(res) = op.identical_optimization(zelf, other) {
                return Ok(res.into());
            }
            let other = class_or_notimplemented!(Self, other);
            let lhs = zelf.borrow_deque();
            let rhs = other.borrow_deque();
            lhs.iter()
                .richcompare(rhs.iter(), op, vm)
                .map(PyComparisonValue::Implemented)
        }
    }

    impl Iterable for PyDeque {
        fn iter(zelf: PyRef<Self>, vm: &VirtualMachine) -> PyResult {
            Ok(PyDequeIterator::new(zelf).into_pyobject(vm))
        }
    }

    impl Representable for PyDeque {
        #[inline]
        fn repr_str(zelf: &Py<Self>, vm: &VirtualMachine) -> PyResult<String> {
            let deque = zelf.borrow_deque().clone();
            let class = zelf.class();
            let class_name = class.name();
            let closing_part = zelf
                .maxlen
                .map(|maxlen| format!("], maxlen={maxlen}"))
                .unwrap_or_else(|| "]".to_owned());

            let s = if zelf.__len__() == 0 {
                format!("{class_name}([{closing_part})")
            } else if let Some(_guard) = ReprGuard::enter(vm, zelf.as_object()) {
                collection_repr(Some(&class_name), "[", &closing_part, deque.iter(), vm)?
            } else {
                "[...]".to_owned()
            };

            Ok(s)
        }
    }

    #[pyattr]
    #[pyclass(name = "_deque_iterator")]
    #[derive(Debug, PyPayload)]
    struct PyDequeIterator {
        state: usize,
        internal: PyMutex<PositionIterInternal<PyDequeRef>>,
    }

    #[derive(FromArgs)]
    struct DequeIterArgs {
        #[pyarg(positional)]
        deque: PyDequeRef,

        #[pyarg(positional, optional)]
        index: OptionalArg<isize>,
    }

    impl Constructor for PyDequeIterator {
        type Args = (DequeIterArgs, KwArgs);

        fn py_new(
            cls: PyTypeRef,
            (DequeIterArgs { deque, index }, _kwargs): Self::Args,
            vm: &VirtualMachine,
        ) -> PyResult {
            let iter = Self::new(deque);
            if let OptionalArg::Present(index) = index {
                let index = max(index, 0) as usize;
                iter.internal.lock().position = index;
            }
            iter.into_ref_with_type(vm, cls).map(Into::into)
        }
    }

    #[pyclass(with(IterNext, Iterable, Constructor))]
    impl PyDequeIterator {
        pub(crate) fn new(deque: PyDequeRef) -> Self {
            Self {
                state: deque.state.load(),
                internal: PyMutex::new(PositionIterInternal::new(deque, 0)),
            }
        }

        #[pymethod]
        fn __length_hint__(&self) -> usize {
            self.internal.lock().length_hint(|obj| obj.__len__())
        }

        #[pymethod]
        fn __reduce__(
            zelf: PyRef<Self>,
            vm: &VirtualMachine,
        ) -> (PyTypeRef, (PyDequeRef, PyObjectRef)) {
            let internal = zelf.internal.lock();
            let deque = match &internal.status {
                Active(obj) => obj.clone(),
                Exhausted => PyDeque::default().into_ref(&vm.ctx),
            };
            (
                zelf.class().to_owned(),
                (deque, vm.ctx.new_int(internal.position).into()),
            )
        }
    }

    impl SelfIter for PyDequeIterator {}
    impl IterNext for PyDequeIterator {
        fn next(zelf: &Py<Self>, vm: &VirtualMachine) -> PyResult<PyIterReturn> {
            zelf.internal.lock().next(|deque, pos| {
                if zelf.state != deque.state.load() {
                    return Err(vm.new_runtime_error("Deque mutated during iteration"));
                }
                let deque = deque.borrow_deque();
                Ok(PyIterReturn::from_result(
                    deque.get(pos).cloned().ok_or(None),
                ))
            })
        }
    }

    #[pyattr]
    #[pyclass(name = "_deque_reverse_iterator")]
    #[derive(Debug, PyPayload)]
    struct PyReverseDequeIterator {
        state: usize,
        // position is counting from the tail
        internal: PyMutex<PositionIterInternal<PyDequeRef>>,
    }

    impl Constructor for PyReverseDequeIterator {
        type Args = (DequeIterArgs, KwArgs);

        fn py_new(
            cls: PyTypeRef,

            (DequeIterArgs { deque, index }, _kwargs): Self::Args,
            vm: &VirtualMachine,
        ) -> PyResult {
            let iter = PyDeque::__reversed__(deque)?;
            if let OptionalArg::Present(index) = index {
                let index = max(index, 0) as usize;
                iter.internal.lock().position = index;
            }
            iter.into_ref_with_type(vm, cls).map(Into::into)
        }
    }

    #[pyclass(with(IterNext, Iterable, Constructor))]
    impl PyReverseDequeIterator {
        #[pymethod]
        fn __length_hint__(&self) -> usize {
            self.internal.lock().length_hint(|obj| obj.__len__())
        }

        #[pymethod]
        fn __reduce__(
            zelf: PyRef<Self>,
            vm: &VirtualMachine,
        ) -> PyResult<(PyTypeRef, (PyDequeRef, PyObjectRef))> {
            let internal = zelf.internal.lock();
            let deque = match &internal.status {
                Active(obj) => obj.clone(),
                Exhausted => PyDeque::default().into_ref(&vm.ctx),
            };
            Ok((
                zelf.class().to_owned(),
                (deque, vm.ctx.new_int(internal.position).into()),
            ))
        }
    }

    impl SelfIter for PyReverseDequeIterator {}
    impl IterNext for PyReverseDequeIterator {
        fn next(zelf: &Py<Self>, vm: &VirtualMachine) -> PyResult<PyIterReturn> {
            zelf.internal.lock().next(|deque, pos| {
                if deque.state.load() != zelf.state {
                    return Err(vm.new_runtime_error("Deque mutated during iteration"));
                }
                let deque = deque.borrow_deque();
                let r = deque
                    .len()
                    .checked_sub(pos + 1)
                    .and_then(|pos| deque.get(pos))
                    .cloned();
                Ok(PyIterReturn::from_result(r.ok_or(None)))
            })
        }
    }
}
