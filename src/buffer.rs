use alloc::boxed::Box;
use alloc::vec::Vec;
use core::any::TypeId;

/// A buffer for reusing allocations between calls to [`Buffer::encode`] and/or [`Buffer::decode`].
///
/// ```rust
/// use bitcode::{Buffer, Encode, Decode};
///
/// let original = "Hello world!";
///
/// let mut buffer = Buffer::new();
/// buffer.encode(&original);
/// let encoded: &[u8] = buffer.encode(&original); // Won't allocate
///
/// let mut buffer = Buffer::new();
/// buffer.decode::<&str>(&encoded).unwrap();
/// let decoded: &str = buffer.decode(&encoded).unwrap(); // Won't allocate
/// assert_eq!(original, decoded);
/// ```
#[derive(Default)]
pub struct Buffer {
    pub(crate) registry: Registry,
    pub(crate) out: Vec<u8>, // Isn't stored in registry because all encoders can share this.
}

impl Buffer {
    /// Constructs a new buffer.
    pub fn new() -> Self {
        Self::default()
    }
}

// Set of arbitrary types.
#[derive(Default)]
pub(crate) struct Registry(Vec<(TypeId, ErasedBox)>);

impl Registry {
    /// Gets a `&mut T` if it already exists or initializes one with [`Default`].
    #[cfg(test)]
    pub(crate) fn get<T: Default + Send + Sync + 'static>(&mut self) -> &mut T {
        // Safety: T is static.
        unsafe { self.get_non_static::<T>() }
    }

    /// Like [`Registry::get`] but can get non-static types.
    /// # Safety
    /// Lifetimes are the responsibility of the caller. `&'static [u8]` and `&'a [u8]` are the same
    /// type from the perspective of this function.
    pub(crate) unsafe fn get_non_static<T: Default + Send + Sync>(&mut self) -> &mut T {
        // Use non-generic function to avoid monomorphization.
        #[inline(never)]
        fn inner(me: &mut Registry, type_id: TypeId, create: fn() -> ErasedBox) -> *mut () {
            // Use sorted Vec + binary search because we expect fewer insertions than lookups.
            // We could use a HashMap, but that seems like overkill.
            match me.0.binary_search_by_key(&type_id, |(k, _)| *k) {
                Ok(i) => me.0[i].1.ptr,
                Err(i) => {
                    #[cold]
                    #[inline(never)]
                    fn cold(
                        me: &mut Registry,
                        i: usize,
                        type_id: TypeId,
                        create: fn() -> ErasedBox,
                    ) -> *mut () {
                        me.0.insert(i, (type_id, create()));
                        me.0[i].1.ptr
                    }
                    cold(me, i, type_id, create)
                }
            }
        }
        let erased_ptr = inner(self, non_static_type_id::<T>(), || {
            // Safety: Caller upholds any lifetime requirements.
            ErasedBox::new(T::default())
        });

        // Safety: type_id uniquely identifies the type, so the entry with equal TypeId is a T.
        &mut *(erased_ptr as *mut T)
    }
}

/// Ignores lifetimes in `T` when determining its [`TypeId`].
/// https://github.com/rust-lang/rust/issues/41875#issuecomment-317292888
fn non_static_type_id<T: ?Sized>() -> TypeId {
    use core::marker::PhantomData;
    trait NonStaticAny {
        fn get_type_id(&self) -> TypeId
        where
            Self: 'static;
    }
    impl<T: ?Sized> NonStaticAny for PhantomData<T> {
        fn get_type_id(&self) -> TypeId
        where
            Self: 'static,
        {
            TypeId::of::<T>()
        }
    }
    let phantom_data = PhantomData::<T>;
    NonStaticAny::get_type_id(unsafe {
        core::mem::transmute::<&dyn NonStaticAny, &(dyn NonStaticAny + 'static)>(&phantom_data)
    })
}

/// `Box<T>` but of an unknown runtime `T`, requires unsafe to get the `T` back out.
struct ErasedBox {
    ptr: *mut (),             // Box<T>
    drop: unsafe fn(*mut ()), // fn(Box<T>)
}

// Safety: `ErasedBox::new` ensures `T: Send + Sync`.
unsafe impl Send for ErasedBox {}
unsafe impl Sync for ErasedBox {}

impl ErasedBox {
    /// Allocates a [`Box<T>`] which doesn't know its own type. Only works on `T: Sized`.
    /// # Safety
    /// Ignores lifetimes so drop may be called after `T`'s lifetime has expired.
    unsafe fn new<T: Send + Sync>(t: T) -> Self {
        let ptr = Box::into_raw(Box::new(t)) as *mut ();
        let drop: unsafe fn(*mut ()) = core::mem::transmute(drop::<Box<T>> as fn(Box<T>));
        Self { ptr, drop }
    }
}

impl Drop for ErasedBox {
    fn drop(&mut self) {
        // Safety: `ErasedBox::new` put a `Box<T>` in self.ptr and an `fn(Box<T>)` in self.drop.
        unsafe { (self.drop)(self.ptr) };
    }
}

#[cfg(test)]
mod tests {
    use super::{non_static_type_id, Buffer, ErasedBox, Registry};
    use test::{black_box, Bencher};

    #[test]
    fn buffer() {
        let mut b = Buffer::new();
        assert_eq!(b.encode(&false), &[0]);
        assert_eq!(b.encode(&true), &[1]);
        assert_eq!(b.decode::<bool>(&[0]).unwrap(), false);
        assert_eq!(b.decode::<bool>(&[1]).unwrap(), true);

        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Buffer>()
    }

    #[test]
    fn registry() {
        let mut r = Registry::default();
        assert_eq!(*r.get::<u8>(), 0);
        *r.get::<u8>() = 1;
        assert_eq!(*r.get::<u8>(), 1);

        assert_eq!(*r.get::<u16>(), 0);
        *r.get::<u16>() = 5;
        assert_eq!(*r.get::<u16>(), 5);

        assert_eq!(*r.get::<u8>(), 1);
    }

    #[test]
    fn type_id() {
        assert_ne!(non_static_type_id::<u8>(), non_static_type_id::<i8>());
        assert_ne!(non_static_type_id::<()>(), non_static_type_id::<[(); 1]>());
        assert_ne!(
            non_static_type_id::<&'static mut [u8]>(),
            non_static_type_id::<&'static [u8]>()
        );
        assert_ne!(
            non_static_type_id::<*mut u8>(),
            non_static_type_id::<*const u8>()
        );
        fn f<'a>(_: &'a ()) {
            assert_eq!(
                non_static_type_id::<&'static [u8]>(),
                non_static_type_id::<&'a [u8]>()
            );
            assert_eq!(
                non_static_type_id::<&'static ()>(),
                non_static_type_id::<&'a ()>()
            );
        }
        f(&());
    }

    #[test]
    fn erased_box() {
        use alloc::sync::Arc;
        let rc = Arc::new(());
        struct TestDrop(#[allow(unused)] Arc<()>);
        let b = unsafe { ErasedBox::new(TestDrop(Arc::clone(&rc))) };
        assert_eq!(Arc::strong_count(&rc), 2);
        drop(b);
        assert_eq!(Arc::strong_count(&rc), 1);
    }

    macro_rules! register10 {
        ($registry:ident $(, $t:literal)*) => {
            $(
                $registry.get::<[u8; $t]>();
                $registry.get::<[i8; $t]>();
                $registry.get::<[u16; $t]>();
                $registry.get::<[i16; $t]>();
                $registry.get::<[u32; $t]>();
                $registry.get::<[i32; $t]>();
                $registry.get::<[u64; $t]>();
                $registry.get::<[i64; $t]>();
                $registry.get::<[u128; $t]>();
                $registry.get::<[i128; $t]>();
            )*
        }
    }
    type T = [u8; 1];

    #[bench]
    fn bench_registry1_get(b: &mut Bencher) {
        let mut r = Registry::default();
        r.get::<T>();
        assert_eq!(r.0.len(), 1);
        b.iter(|| {
            black_box(*black_box(&mut r).get::<T>());
        })
    }

    #[bench]
    fn bench_registry10_get(b: &mut Bencher) {
        let mut r = Registry::default();
        r.get::<T>();
        register10!(r, 1);
        assert_eq!(r.0.len(), 10);
        b.iter(|| {
            black_box(*black_box(&mut r).get::<T>());
        })
    }

    #[bench]
    fn bench_registry100_get(b: &mut Bencher) {
        let mut r = Registry::default();
        r.get::<T>();
        register10!(r, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10);
        assert_eq!(r.0.len(), 100);
        b.iter(|| {
            black_box(*black_box(&mut r).get::<T>());
        })
    }
}
