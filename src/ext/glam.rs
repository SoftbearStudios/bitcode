use super::impl_struct;
use glam::*;

trait Affine3AExt {
    fn from_mat3a_translation(matrix3: Mat3A, translation: Vec3A) -> Self;
}
impl Affine3AExt for Affine3A {
    fn from_mat3a_translation(matrix3: Mat3A, translation: Vec3A) -> Self {
        Self { matrix3, translation }
    }
}
impl_struct!(Affine2, from_mat2_translation, matrix2, Mat2, translation, Vec2);
impl_struct!(DAffine2, from_mat2_translation, matrix2, DMat2, translation, DVec2);
impl_struct!(Affine3A, from_mat3a_translation, matrix3, Mat3A, translation, Vec3A);
impl_struct!(DAffine3, from_mat3_translation, matrix3, DMat3, translation, DVec3);

macro_rules! impl_vec {
    ($t:ident, $new:ident, $e:ty, $($f:ident),+) => {
        impl_struct!($t, $new, $($f, $e),+);
    }
}
impl_vec!(Vec3A, new, f32, x, y, z);
impl_vec!(Mat3A, from_cols, Vec3A, x_axis, y_axis, z_axis);

macro_rules! impl_glam {
    ($e:ty, $v2:ident, $v3:ident, $v4:ident $(, $q:ident, $m2:ident, $m3:ident, $m4:ident)?) => {
        impl_vec!($v2, new, $e, x, y);
        impl_vec!($v3, new, $e, x, y, z);
        impl_vec!($v4, new, $e, x, y, z, w);
        $(
            impl_vec!($q, from_xyzw, $e, x, y, z, w);
            impl_vec!($m2, from_cols, $v2, x_axis, y_axis);
            impl_vec!($m3, from_cols, $v3, x_axis, y_axis, z_axis);
            impl_vec!($m4, from_cols, $v4, x_axis, y_axis, z_axis, w_axis);
        )?
    }
}
impl_glam!(f32, Vec2, Vec3, Vec4, Quat, Mat2, Mat3, Mat4);
impl_glam!(f64, DVec2, DVec3, DVec4, DQuat, DMat2, DMat3, DMat4);
impl_glam!(u32, UVec2, UVec3, UVec4);
impl_glam!(i32, IVec2, IVec3, IVec4);
impl_glam!(bool, BVec2, BVec3, BVec4);
