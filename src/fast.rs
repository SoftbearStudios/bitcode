use std::marker::PhantomData;
use std::mem::{ManuallyDrop, MaybeUninit};

pub type VecImpl<T> = FastVec<T>;
pub type SliceImpl<'a, T> = FastSlice<'a, T>;

/// Implementation of [`Vec`] that optimizes push_unchecked at the cost of as_slice being slower.
pub struct FastVec<T> {
    start: *mut T,    // vec.as_mut_ptr()
    end: *mut T,      // vec.as_mut_ptr().add(vec.len())
    capacity: *mut T, // vec.as_mut_ptr().add(vec.capacity())
    _spooky: PhantomData<Vec<T>>,
}

impl<T> Default for FastVec<T> {
    fn default() -> Self {
        Self::from(vec![])
    }
}

impl<T> Drop for FastVec<T> {
    fn drop(&mut self) {
        unsafe {
            drop(Vec::from(std::ptr::read(self)));
        }
    }
}

// Safety: Same bounds as [`Vec`] impls.
unsafe impl<T: Send> Send for FastVec<T> {}
unsafe impl<T: Sync> Sync for FastVec<T> {}

/// Replacement for `feature = "ptr_sub_ptr"` which isn't yet stable.
#[inline(always)]
fn sub_ptr<T>(ptr: *mut T, origin: *mut T) -> usize {
    // unsafe { ptr.sub_ptr(origin) }
    (ptr as usize - origin as usize) / std::mem::size_of::<T>()
}

impl<T> From<FastVec<T>> for Vec<T> {
    fn from(fast: FastVec<T>) -> Self {
        let start = fast.start;
        let length = fast.len();
        let capacity = sub_ptr(fast.capacity, fast.start);
        std::mem::forget(fast);
        unsafe { Vec::from_raw_parts(start, length, capacity) }
    }
}

impl<T> From<Vec<T>> for FastVec<T> {
    fn from(mut vec: Vec<T>) -> Self {
        assert_ne!(std::mem::size_of::<T>(), 0);
        let start = vec.as_mut_ptr();
        let end = unsafe { start.add(vec.len()) };
        let capacity = unsafe { start.add(vec.capacity()) };
        std::mem::forget(vec);
        Self {
            start,
            end,
            capacity,
            _spooky: Default::default(),
        }
    }
}

impl<T> FastVec<T> {
    pub fn len(&self) -> usize {
        sub_ptr(self.end, self.start)
    }

    pub fn as_slice(&self) -> &[T] {
        unsafe { std::slice::from_raw_parts(self.start, self.len()) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { std::slice::from_raw_parts_mut(self.start, self.len()) }
    }

    pub fn clear(&mut self) {
        // Safety: same as `Vec::clear` except `self.end = self.start` instead of `self.len = 0` but
        // these are equivalent operations. Can't use `self.mut_vec(Vec::clear)` because T::drop
        // panicking would double free elements.
        unsafe {
            let elems: *mut [T] = self.as_mut_slice();
            self.end = self.start;
            std::ptr::drop_in_place(elems);
        }
    }

    pub fn reserve(&mut self, additional: usize) {
        if additional > sub_ptr(self.capacity, self.end) {
            #[cold]
            #[inline(never)]
            fn reserve_slow<T>(me: &mut FastVec<T>, additional: usize) {
                // Safety: `Vec::reserve` panics on OOM without freeing Vec, so Vec is unmodified.
                unsafe {
                    me.mut_vec(|v| {
                        // Optimizes out a redundant check in `Vec::reserve`.
                        // Safety: we've already ensured this condition before calling reserve_slow.
                        if additional <= v.capacity().wrapping_sub(v.len()) {
                            std::hint::unreachable_unchecked();
                        }
                        v.reserve(additional);
                    });
                }
            }
            reserve_slow(self, additional);
        }
    }

    /// Accesses the [`FastVec`] mutably as a [`Vec`].
    /// # Safety
    /// If `f` panics the [`Vec`] must be unmodified.
    unsafe fn mut_vec(&mut self, f: impl FnOnce(&mut Vec<T>)) {
        let copied = std::ptr::read(self as *mut FastVec<T>);
        let mut vec = ManuallyDrop::new(Vec::from(copied));
        f(&mut vec);
        let copied = FastVec::from(ManuallyDrop::into_inner(vec));
        std::ptr::write(self as *mut FastVec<T>, copied);
    }

    /// Get a pointer to write to without incrementing length.
    #[inline(always)]
    pub fn end_ptr(&mut self) -> *mut T {
        debug_assert!(self.end <= self.capacity);
        self.end
    }

    /// Set the end_ptr after mutating it.
    #[inline(always)]
    pub fn set_end_ptr(&mut self, end: *mut T) {
        self.end = end;
        debug_assert!(self.end <= self.capacity);
    }

    /// Increments length by 1.
    ///
    /// Safety:
    ///
    /// Element at [`Self::end_ptr()`] must have been initialized.
    #[inline(always)]
    pub unsafe fn increment_len(&mut self) {
        self.end = self.end.add(1);
        debug_assert!(self.end <= self.capacity);
    }
}

pub trait PushUnchecked<T> {
    /// Like [`Vec::push`] but without the possibility of allocating.
    /// Safety: len must be < capacity.
    unsafe fn push_unchecked(&mut self, t: T);
}

impl<T> PushUnchecked<T> for FastVec<T> {
    #[inline(always)]
    unsafe fn push_unchecked(&mut self, t: T) {
        debug_assert!(self.end < self.capacity);
        std::ptr::write(self.end, t);
        self.end = self.end.add(1);
    }
}

impl<T> PushUnchecked<T> for Vec<T> {
    #[inline(always)]
    unsafe fn push_unchecked(&mut self, t: T) {
        let n = self.len();
        debug_assert!(n < self.capacity());
        let end = self.as_mut_ptr().add(n);
        std::ptr::write(end, t);
        self.set_len(n + 1);
    }
}

/// Like [`FastVec`] but borrows a [`MaybeUninit<[T; N]>`] instead of heap allocating. Only accepts
/// `T: Copy` because it doesn't drop elements.
pub struct FastArrayVec<'a, T: Copy, const N: usize> {
    start: *mut T,
    end: *mut T,
    _spooky: PhantomData<&'a mut T>,
}

impl<'a, T: Copy, const N: usize> FastArrayVec<'a, T, N> {
    #[inline(always)]
    pub fn new(uninit: &'a mut MaybeUninit<[T; N]>) -> Self {
        let start = uninit.as_mut_ptr() as *mut T;
        Self {
            start,
            end: start,
            _spooky: PhantomData,
        }
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &[T] {
        let len = sub_ptr(self.end, self.start);
        unsafe { std::slice::from_raw_parts(self.start, len) }
    }
}

impl<'a, T: Copy, const N: usize> PushUnchecked<T> for FastArrayVec<'a, T, N> {
    #[inline(always)]
    unsafe fn push_unchecked(&mut self, t: T) {
        std::ptr::write(self.end, t);
        self.end = self.end.add(1);
    }
}

#[derive(Clone)]
pub struct FastSlice<'a, T> {
    ptr: *const T,
    #[cfg(debug_assertions)]
    end: *const T, // Uses pointer instead of len to permit &mut FastSlice<T> -> &mut FastSlice<[T; N]> cast.
    _spooky: PhantomData<&'a T>,
}

impl<T> Default for FastSlice<'_, T> {
    fn default() -> Self {
        Self::from([].as_slice())
    }
}

// Safety: Same bounds as slice impls.
unsafe impl<T: Send> Send for FastSlice<'_, T> {}
unsafe impl<T: Sync> Sync for FastSlice<'_, T> {}

impl<'a, T> From<&'a [T]> for FastSlice<'a, T> {
    fn from(slice: &'a [T]) -> Self {
        Self {
            ptr: slice.as_ptr(),
            #[cfg(debug_assertions)]
            end: slice.as_ptr_range().end,
            _spooky: PhantomData,
        }
    }
}

impl<'a, T> FastSlice<'a, T> {
    /// Safety: `ptr` and `len` must form a valid slice.
    #[inline(always)]
    pub unsafe fn from_raw_parts(ptr: *const T, len: usize) -> Self {
        let _ = len;
        Self {
            ptr,
            #[cfg(debug_assertions)]
            end: ptr.wrapping_add(len),
            _spooky: PhantomData,
        }
    }

    /// Like [`NextUnchecked::next_unchecked`] but doesn't dereference the `T`.
    #[inline(always)]
    pub unsafe fn next_unchecked_as_ptr(&mut self) -> *const T {
        #[cfg(debug_assertions)]
        assert!((self.ptr.wrapping_add(1) as usize) <= self.end as usize);
        let p = self.ptr;
        self.ptr = self.ptr.add(1);
        p
    }

    #[inline(always)]
    pub unsafe fn advance(&mut self, n: usize) {
        #[cfg(debug_assertions)]
        assert!((self.ptr.wrapping_add(n) as usize) <= self.end as usize);
        self.ptr = self.ptr.add(n);
    }

    #[inline(always)]
    pub fn as_ptr(&self) -> *const T {
        self.ptr
    }

    /// Casts `&mut FastSlice<T>` to `&mut FastSlice<B>`.
    #[inline(always)]
    pub fn cast<B>(&mut self) -> &mut FastSlice<'a, B>
    where
        T: bytemuck::Pod,
        B: bytemuck::Pod,
    {
        use std::mem::*;
        assert_eq!(size_of::<T>(), size_of::<B>());
        assert_eq!(align_of::<T>(), align_of::<B>());
        // Safety: size/align are equal and both are bytemuck::Pod.
        unsafe { transmute(self) }
    }
}

pub trait NextUnchecked<'a, T: Copy> {
    /// Gets the next item out of the slice and sets the slice to the remaining elements.
    /// Safety: can only call len times.
    unsafe fn next_unchecked(&mut self) -> T;

    /// Consumes `length` elements of the slice.
    /// Safety: length must be in bounds.
    unsafe fn chunk_unchecked(&mut self, length: usize) -> &'a [T];
}

impl<'a, T: Copy> NextUnchecked<'a, T> for FastSlice<'a, T> {
    #[inline(always)]
    unsafe fn next_unchecked(&mut self) -> T {
        #[cfg(debug_assertions)]
        assert!((self.ptr.wrapping_add(1) as usize) <= self.end as usize);
        let t = *self.ptr;
        self.ptr = self.ptr.add(1);
        t
    }

    #[inline(always)]
    unsafe fn chunk_unchecked(&mut self, length: usize) -> &'a [T] {
        #[cfg(debug_assertions)]
        assert!((self.ptr.wrapping_add(length) as usize) <= self.end as usize);
        let slice = std::slice::from_raw_parts(self.ptr, length);
        self.ptr = self.ptr.add(length);
        slice
    }
}

impl<'a, T: Copy> NextUnchecked<'a, T> for &'a [T] {
    #[inline(always)]
    unsafe fn next_unchecked(&mut self) -> T {
        let p = *self.get_unchecked(0);
        *self = self.get_unchecked(1..);
        p
    }

    #[inline(always)]
    unsafe fn chunk_unchecked(&mut self, length: usize) -> &'a [T] {
        let slice = self.get_unchecked(0..length);
        *self = self.get_unchecked(length..);
        slice
    }
}

/// Maybe owned [`FastSlice`]. Saves its allocation even if borrowing something.
#[derive(Default)]
pub struct CowSlice<'borrowed, T> {
    slice: SliceImpl<'borrowed, T>, // Lifetime is min of 'borrowed and &'me self.
    vec: Vec<T>,
}
impl<'borrowed, T> CowSlice<'borrowed, T> {
    /// Creates a [`CowSlice`] with an allocation of `vec`. None of `vec`'s elements are kept.
    pub fn with_allocation(mut vec: Vec<T>) -> Self {
        vec.clear();
        Self {
            slice: [].as_slice().into(),
            vec,
        }
    }

    /// Converts a [`CowSlice`] into its internal allocation. The [`Vec<T>`] is empty.
    pub fn into_allocation(mut self) -> Vec<T> {
        self.vec.clear();
        self.vec
    }

    /// References the inner [`SliceImpl`] as a `[T]`.
    /// Safety: `len` must be equal to the slices original len.
    #[must_use]
    pub unsafe fn as_slice<'me>(&'me self, len: usize) -> &'me [T]
    where
        'borrowed: 'me,
    {
        #[cfg(debug_assertions)]
        assert_eq!(self.slice.ptr.wrapping_add(len), self.slice.end);
        std::slice::from_raw_parts(self.slice.ptr, len)
    }

    /// References the inner [`SliceImpl`].
    #[must_use]
    #[inline(always)]
    pub fn ref_slice<'me>(&'me self) -> &'me SliceImpl<'me, T>
    where
        'borrowed: 'me,
    {
        // Safety: 'me is min of 'borrowed and &'me self because of `where 'borrowed: 'me`.
        let slice: &'me SliceImpl<'me, T> = unsafe { std::mem::transmute(&self.slice) };
        slice
    }

    /// Mutates the inner [`SliceImpl`].
    #[must_use]
    #[inline(always)]
    pub fn mut_slice<'me>(&'me mut self) -> &'me mut SliceImpl<'me, T>
    where
        'borrowed: 'me,
    {
        // Safety: 'me is min of 'borrowed and &'me self because of `where 'borrowed: 'me`.
        let slice: &'me mut SliceImpl<'me, T> = unsafe { std::mem::transmute(&mut self.slice) };
        slice
    }

    /// Equivalent to `self.set_owned().extend_from_slice(slice)` but without copying.
    pub fn set_borrowed(&mut self, slice: &'borrowed [T]) {
        self.slice = slice.into();
    }

    /// Equivalent to [`Self::set_borrowed`] but takes a [`SliceImpl`] instead of a `&[T]`.
    pub fn set_borrowed_slice_impl(&mut self, slice: SliceImpl<'borrowed, T>) {
        self.slice = slice;
    }

    /// Allows putting contents into a cleared `&mut Vec<T>`. When `SetOwned` is dropped the
    /// `CowSlice` will be updated to reference the new elements.
    #[must_use]
    pub fn set_owned(&mut self) -> SetOwned<'_, 'borrowed, T> {
        // Clear self.slice before mutating self.vec, so we don't point to freed memory.
        self.slice = [].as_slice().into();
        self.vec.clear();
        SetOwned(self)
    }

    /// Mutates the owned [`Vec<T>`].
    ///
    /// **Panics**
    ///
    /// If self is not owned (set_owned hasn't been called).
    pub fn mut_owned<R>(&mut self, f: impl FnOnce(&mut Vec<T>) -> R) -> R {
        assert!(std::ptr::eq(self.slice.ptr, self.vec.as_ptr()), "not owned");
        // Clear self.slice before mutating self.vec, so we don't point to freed memory.
        self.slice = [].as_slice().into();
        let ret = f(&mut self.vec);
        // Safety: We clear `CowSlice.slice` whenever we mutate `CowSlice.vec`.
        let slice: &'borrowed [T] = unsafe { std::mem::transmute(self.vec.as_slice()) };
        self.slice = slice.into();
        ret
    }

    /// Casts `&mut CowSlice<T>` to `&mut CowSlice<B>`.
    #[inline]
    pub fn cast_mut<B>(&mut self) -> &mut CowSlice<'borrowed, B>
    where
        T: bytemuck::Pod,
        B: bytemuck::Pod,
    {
        use std::mem::*;
        assert_eq!(size_of::<T>(), size_of::<B>());
        assert_eq!(align_of::<T>(), align_of::<B>());
        // Safety: size/align are equal and both are bytemuck::Pod.
        unsafe { transmute(self) }
    }
}

pub struct SetOwned<'a, 'borrowed, T>(&'a mut CowSlice<'borrowed, T>);
impl<'borrowed, T> Drop for SetOwned<'_, 'borrowed, T> {
    fn drop(&mut self) {
        // Safety: We clear `CowSlice.slice` whenever we mutate `CowSlice.vec`.
        let slice: &'borrowed [T] = unsafe { std::mem::transmute(self.0.vec.as_slice()) };
        self.0.slice = slice.into();
    }
}
impl<'a, T> std::ops::Deref for SetOwned<'a, '_, T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.0.vec
    }
}
impl<'a, T> std::ops::DerefMut for SetOwned<'a, '_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0.vec
    }
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct Unaligned<T>(T);

// Could derive with bytemuck/derive.
unsafe impl<T: bytemuck::Zeroable> bytemuck::Zeroable for Unaligned<T> {}
unsafe impl<T: bytemuck::Pod> bytemuck::Pod for Unaligned<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use test::{black_box, Bencher};

    #[test]
    fn test_as_slice() {
        let mut vec = FastVec::default();
        vec.reserve(2);
        unsafe {
            vec.push_unchecked(1);
            vec.push_unchecked(2);
        }
        assert_eq!(vec.as_slice(), [1, 2]);
    }

    const N: usize = 1000;
    type VecT = Vec<u32>;

    #[bench]
    fn bench_next_unchecked(b: &mut Bencher) {
        let src: VecT = vec![0; N];
        b.iter(|| {
            let mut slice = src.as_slice();
            for _ in 0..black_box(N) {
                unsafe { black_box(black_box(&mut slice).next_unchecked()) };
            }
        });
    }

    #[bench]
    fn bench_next_unchecked_fast(b: &mut Bencher) {
        let src: VecT = vec![0; N];
        b.iter(|| {
            let mut fast_slice = FastSlice::from(src.as_slice());
            for _ in 0..black_box(N) {
                unsafe { black_box(black_box(&mut fast_slice).next_unchecked()) };
            }
        });
    }

    #[bench]
    fn bench_push(b: &mut Bencher) {
        let mut buffer = VecT::with_capacity(N);
        b.iter(|| {
            buffer.clear();
            let vec = black_box(&mut buffer);
            for _ in 0..black_box(N) {
                let v = black_box(&mut *vec);
                v.push(black_box(0));
            }
        });
    }

    #[bench]
    fn bench_push_fast(b: &mut Bencher) {
        let mut buffer = VecT::with_capacity(N);
        b.iter(|| {
            buffer.clear();
            let mut vec = black_box(FastVec::from(std::mem::take(&mut buffer)));
            for _ in 0..black_box(N) {
                let v = black_box(&mut vec);
                v.reserve(1);
                unsafe { v.push_unchecked(black_box(0)) };
            }
            buffer = vec.into();
        });
    }

    #[bench]
    fn bench_push_unchecked(b: &mut Bencher) {
        let mut buffer = VecT::with_capacity(N);
        b.iter(|| {
            buffer.clear();
            let vec = black_box(&mut buffer);
            for _ in 0..black_box(N) {
                let v = black_box(&mut *vec);
                unsafe { v.push_unchecked(black_box(0)) };
            }
        });
    }

    #[bench]
    fn bench_push_unchecked_fast(b: &mut Bencher) {
        let mut buffer = VecT::with_capacity(N);
        b.iter(|| {
            buffer.clear();
            let mut vec = black_box(FastVec::from(std::mem::take(&mut buffer)));
            for _ in 0..black_box(N) {
                let v = black_box(&mut vec);
                unsafe { v.push_unchecked(black_box(0)) };
            }
            buffer = vec.into();
        });
    }

    #[bench]
    fn bench_reserve(b: &mut Bencher) {
        let mut buffer = VecT::with_capacity(N);
        b.iter(|| {
            buffer.clear();
            let vec = black_box(&mut buffer);
            for _ in 0..black_box(N) {
                black_box(&mut *vec).reserve(1);
            }
        });
    }

    #[bench]
    fn bench_reserve_fast(b: &mut Bencher) {
        let mut buffer = VecT::with_capacity(N);
        b.iter(|| {
            buffer.clear();
            let mut vec = black_box(FastVec::from(std::mem::take(&mut buffer)));
            for _ in 0..black_box(N) {
                black_box(&mut vec).reserve(1);
            }
            buffer = vec.into();
        });
    }
}
