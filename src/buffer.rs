use std::any::TypeId;

/// A buffer for reusing allocations between calls to [`Buffer::encode`] and/or [`Buffer::decode`].
/// TODO Send + Sync
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
    pub(crate) fn get<T: Default + 'static>(&mut self) -> &mut T {
        // Safety: T is static.
        unsafe { self.get_non_static::<T>() }
    }

    /// Like [`Registry::get`] but can get non-static types.
    /// # Safety
    /// Lifetimes are the responsibility of the caller. `&'static [u8]` and `&'a [u8]` are the same
    /// type from the perspective of this function.
    pub(crate) unsafe fn get_non_static<T: Default>(&mut self) -> &mut T {
        // Use sorted Vec + binary search because we expect fewer insertions than lookups.
        // We could use a HashMap, but that seems like overkill.
        let type_id = non_static_type_id::<T>();
        let i = match self.0.binary_search_by_key(&type_id, |(k, _)| *k) {
            Ok(i) => i,
            Err(i) => {
                #[cold]
                #[inline(never)]
                unsafe fn cold<T: Default>(me: &mut Registry, i: usize) {
                    let type_id = non_static_type_id::<T>();
                    // Safety: caller of `Registry::get` upholds any lifetime requirements.
                    let erased = ErasedBox::new(T::default());
                    me.0.insert(i, (type_id, erased));
                }
                cold::<T>(self, i);
                i
            }
        };
        // Safety: binary_search_by_key either found item at `i` or cold initialized item at `i`.
        let item = &mut self.0.get_unchecked_mut(i).1;
        // Safety: type_id uniquely identifies the type, so the entry with equal type_id is a T.
        item.cast_unchecked_mut()
    }
}

/// Ignores lifetimes in `T` when determining its [`TypeId`].
/// https://github.com/rust-lang/rust/issues/41875#issuecomment-317292888
fn non_static_type_id<T: ?Sized>() -> TypeId {
    use std::marker::PhantomData;
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
        std::mem::transmute::<&dyn NonStaticAny, &(dyn NonStaticAny + 'static)>(&phantom_data)
    })
}

/// `Box<T>` but of an unknown runtime `T`, requires unsafe to get the `T` back out.
struct ErasedBox {
    ptr: *mut (),    // Box<T>
    drop: *const (), // unsafe fn(*mut Box<T>)
}

impl ErasedBox {
    /// Allocates a [`Box<T>`] which doesn't know its own type. Only works on `T: Sized`.
    /// # Safety
    /// Ignores lifetimes so drop may be called after `T`'s lifetime has expired.
    unsafe fn new<T>(t: T) -> Self {
        let ptr = Box::into_raw(Box::new(t)) as *mut ();
        let drop = std::ptr::drop_in_place::<Box<T>> as *const ();
        Self { ptr, drop }
    }

    /// Casts to a `&mut T`.
    /// # Safety
    /// `T` must be the same `T` passed to [`ErasedBox::new`].
    unsafe fn cast_unchecked_mut<T>(&mut self) -> &mut T {
        &mut *(self.ptr as *mut T)
    }
}

impl Drop for ErasedBox {
    fn drop(&mut self) {
        // Safety: `ErasedBox::new` put a `Box<T>` in self.ptr and an `unsafe fn(*mut Box<T>)` in self.drop.
        unsafe {
            let drop: unsafe fn(*mut *mut ()) = std::mem::transmute(self.drop);
            drop((&mut self.ptr) as *mut *mut ()); // Pass *mut Box<T>.
        }
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
        use std::rc::Rc;
        let rc = Rc::new(());
        struct TestDrop(Rc<()>);
        let b = unsafe { ErasedBox::new(TestDrop(Rc::clone(&rc))) };
        assert_eq!(Rc::strong_count(&rc), 2);
        drop(b);
        assert_eq!(Rc::strong_count(&rc), 1);
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
