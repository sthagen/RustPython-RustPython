pub(crate) use hashlib::make_module;

#[pymodule]
mod hashlib {
    use crate::common::lock::{PyRwLock, PyRwLockReadGuard, PyRwLockWriteGuard};
    use crate::vm::{
        builtins::{PyBytes, PyStrRef, PyTypeRef},
        function::{ArgBytesLike, FuncArgs, OptionalArg},
        PyPayload, PyResult, VirtualMachine,
    };
    use blake2::{Blake2b512, Blake2s256};
    use digest::{core_api::BlockSizeUser, DynDigest};
    use dyn_clone::{clone_trait_object, DynClone};
    use md5::Md5;
    use sha1::Sha1;
    use sha2::{Sha224, Sha256, Sha384, Sha512};
    use sha3::{Sha3_224, Sha3_256, Sha3_384, Sha3_512}; // TODO: , shake_128, shake_256;

    #[derive(FromArgs)]
    #[allow(unused)]
    struct NewHashArgs {
        #[pyarg(positional)]
        name: PyStrRef,
        #[pyarg(any, optional)]
        data: OptionalArg<ArgBytesLike>,
        #[pyarg(named, default = "true")]
        usedforsecurity: bool,
    }

    #[derive(FromArgs)]
    #[allow(unused)]
    struct BlakeHashArgs {
        #[pyarg(positional, optional)]
        data: OptionalArg<ArgBytesLike>,
        #[pyarg(named, default = "true")]
        usedforsecurity: bool,
    }

    #[derive(FromArgs)]
    #[allow(unused)]
    struct HashArgs {
        #[pyarg(any, optional)]
        string: OptionalArg<ArgBytesLike>,
        #[pyarg(named, default = "true")]
        usedforsecurity: bool,
    }

    #[pyattr]
    #[pyclass(module = "hashlib", name = "hasher")]
    #[derive(PyPayload)]
    struct PyHasher {
        name: String,
        buffer: PyRwLock<HashWrapper>,
    }

    impl std::fmt::Debug for PyHasher {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "hasher {}", self.name)
        }
    }

    #[pyclass]
    impl PyHasher {
        fn new(name: &str, d: HashWrapper) -> Self {
            PyHasher {
                name: name.to_owned(),
                buffer: PyRwLock::new(d),
            }
        }

        fn read(&self) -> PyRwLockReadGuard<'_, HashWrapper> {
            self.buffer.read()
        }

        fn write(&self) -> PyRwLockWriteGuard<'_, HashWrapper> {
            self.buffer.write()
        }

        #[pyslot]
        fn slot_new(_cls: PyTypeRef, _args: FuncArgs, vm: &VirtualMachine) -> PyResult {
            Ok(PyHasher::new("md5", HashWrapper::new::<Md5>()).into_pyobject(vm))
        }

        #[pygetset]
        fn name(&self) -> String {
            self.name.clone()
        }

        #[pygetset]
        fn digest_size(&self) -> usize {
            self.read().digest_size()
        }

        #[pygetset]
        fn block_size(&self) -> usize {
            self.read().block_size()
        }

        #[pymethod]
        fn update(&self, data: ArgBytesLike) {
            data.with_ref(|bytes| self.write().input(bytes));
        }

        #[pymethod]
        fn digest(&self) -> PyBytes {
            self.get_digest().into()
        }

        #[pymethod]
        fn hexdigest(&self) -> String {
            let result = self.get_digest();
            hex::encode(result)
        }

        #[pymethod]
        fn copy(&self) -> Self {
            PyHasher::new(&self.name, self.buffer.read().clone())
        }

        fn get_digest(&self) -> Vec<u8> {
            self.read().get_digest()
        }
    }

    #[pyfunction(name = "new")]
    fn hashlib_new(args: NewHashArgs, vm: &VirtualMachine) -> PyResult<PyHasher> {
        match args.name.as_str() {
            "md5" => md5(HashArgs {
                string: args.data,
                usedforsecurity: args.usedforsecurity,
            }),
            "sha1" => sha1(HashArgs {
                string: args.data,
                usedforsecurity: args.usedforsecurity,
            }),
            "sha224" => sha224(HashArgs {
                string: args.data,
                usedforsecurity: args.usedforsecurity,
            }),
            "sha256" => sha256(HashArgs {
                string: args.data,
                usedforsecurity: args.usedforsecurity,
            }),
            "sha384" => sha384(HashArgs {
                string: args.data,
                usedforsecurity: args.usedforsecurity,
            }),
            "sha512" => sha512(HashArgs {
                string: args.data,
                usedforsecurity: args.usedforsecurity,
            }),
            "sha3_224" => sha3_224(HashArgs {
                string: args.data,
                usedforsecurity: args.usedforsecurity,
            }),
            "sha3_256" => sha3_256(HashArgs {
                string: args.data,
                usedforsecurity: args.usedforsecurity,
            }),
            "sha3_384" => sha3_384(HashArgs {
                string: args.data,
                usedforsecurity: args.usedforsecurity,
            }),
            "sha3_512" => sha3_512(HashArgs {
                string: args.data,
                usedforsecurity: args.usedforsecurity,
            }),
            // TODO: "shake_128" => shake_128(args.data, ),
            // TODO: "shake_256" => shake_256(args.data, ),
            "blake2b" => blake2b(BlakeHashArgs {
                data: args.data,
                usedforsecurity: args.usedforsecurity,
            }),
            "blake2s" => blake2s(BlakeHashArgs {
                data: args.data,
                usedforsecurity: args.usedforsecurity,
            }),
            other => Err(vm.new_value_error(format!("Unknown hashing algorithm: {other}"))),
        }
    }

    fn init(hasher: PyHasher, data: OptionalArg<ArgBytesLike>) -> PyResult<PyHasher> {
        if let OptionalArg::Present(data) = data {
            hasher.update(data);
        }

        Ok(hasher)
    }

    #[pyfunction]
    fn md5(args: HashArgs) -> PyResult<PyHasher> {
        init(PyHasher::new("md5", HashWrapper::new::<Md5>()), args.string)
    }

    #[pyfunction]
    fn sha1(args: HashArgs) -> PyResult<PyHasher> {
        init(
            PyHasher::new("sha1", HashWrapper::new::<Sha1>()),
            args.string,
        )
    }

    #[pyfunction]
    fn sha224(args: HashArgs) -> PyResult<PyHasher> {
        init(
            PyHasher::new("sha224", HashWrapper::new::<Sha224>()),
            args.string,
        )
    }

    #[pyfunction]
    fn sha256(args: HashArgs) -> PyResult<PyHasher> {
        init(
            PyHasher::new("sha256", HashWrapper::new::<Sha256>()),
            args.string,
        )
    }

    #[pyfunction]
    fn sha384(args: HashArgs) -> PyResult<PyHasher> {
        init(
            PyHasher::new("sha384", HashWrapper::new::<Sha384>()),
            args.string,
        )
    }

    #[pyfunction]
    fn sha512(args: HashArgs) -> PyResult<PyHasher> {
        init(
            PyHasher::new("sha512", HashWrapper::new::<Sha512>()),
            args.string,
        )
    }

    #[pyfunction]
    fn sha3_224(args: HashArgs) -> PyResult<PyHasher> {
        init(
            PyHasher::new("sha3_224", HashWrapper::new::<Sha3_224>()),
            args.string,
        )
    }

    #[pyfunction]
    fn sha3_256(args: HashArgs) -> PyResult<PyHasher> {
        init(
            PyHasher::new("sha3_256", HashWrapper::new::<Sha3_256>()),
            args.string,
        )
    }

    #[pyfunction]
    fn sha3_384(args: HashArgs) -> PyResult<PyHasher> {
        init(
            PyHasher::new("sha3_384", HashWrapper::new::<Sha3_384>()),
            args.string,
        )
    }

    #[pyfunction]
    fn sha3_512(args: HashArgs) -> PyResult<PyHasher> {
        init(
            PyHasher::new("sha3_512", HashWrapper::new::<Sha3_512>()),
            args.string,
        )
    }

    #[pyfunction]
    fn shake_128(_args: HashArgs, vm: &VirtualMachine) -> PyResult<PyHasher> {
        Err(vm.new_not_implemented_error("shake_256".to_owned()))
    }

    #[pyfunction]
    fn shake_256(_args: HashArgs, vm: &VirtualMachine) -> PyResult<PyHasher> {
        Err(vm.new_not_implemented_error("shake_256".to_owned()))
    }

    #[pyfunction]
    fn blake2b(args: BlakeHashArgs) -> PyResult<PyHasher> {
        // TODO: handle parameters
        init(
            PyHasher::new("blake2b", HashWrapper::new::<Blake2b512>()),
            args.data,
        )
    }

    #[pyfunction]
    fn blake2s(args: BlakeHashArgs) -> PyResult<PyHasher> {
        // TODO: handle parameters
        init(
            PyHasher::new("blake2s", HashWrapper::new::<Blake2s256>()),
            args.data,
        )
    }

    trait ThreadSafeDynDigest: DynClone + DynDigest + Sync + Send {}
    impl<T> ThreadSafeDynDigest for T where T: DynClone + DynDigest + Sync + Send {}

    clone_trait_object!(ThreadSafeDynDigest);

    /// Generic wrapper patching around the hashing libraries.
    #[derive(Clone)]
    struct HashWrapper {
        block_size: usize,
        inner: Box<dyn ThreadSafeDynDigest>,
    }

    impl HashWrapper {
        fn new<D>() -> Self
        where
            D: ThreadSafeDynDigest + BlockSizeUser + Default + 'static,
        {
            HashWrapper {
                block_size: D::block_size(),
                inner: Box::<D>::default(),
            }
        }

        fn input(&mut self, data: &[u8]) {
            self.inner.update(data);
        }

        fn block_size(&self) -> usize {
            self.block_size
        }

        fn digest_size(&self) -> usize {
            self.inner.output_size()
        }

        fn get_digest(&self) -> Vec<u8> {
            let cloned = self.inner.box_clone();
            cloned.finalize().into_vec()
        }
    }
}
