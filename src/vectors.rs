#![allow(dead_code)]

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, Zeroable, Pod, PartialEq)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

impl Vector2 {
    pub const ZERO: Self = Self::new(0., 0.);
    pub const UP: Self = Self::new(0., 1.);
    pub const DOWN: Self = Self::new(0., -1.);
    pub const LEFT: Self = Self::new(-1., 0.);
    pub const RIGHT: Self = Self::new(1., 0.);

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub const fn same(x: f32) -> Self {
        Self::new(x, x)
    }

    pub fn length_squared(&self) -> f32 {
        self.dot(*self)
    }
    pub fn length(&self) -> f32 {
        self.length_squared().sqrt()
    }
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y
    }
}

impl From<(f32, f32)> for Vector2 {
    fn from((x, y): (f32, f32)) -> Self {
        Self::new(x, y)
    }
}

impl From<[f32; 2]> for Vector2 {
    fn from([x, y]: [f32; 2]) -> Self {
        Self::new(x, y)
    }
}

macro_rules! vec2_op_impl {
    (self_normal $($trait_name: ident $func_name: ident $op: tt)*) => {
        $(
        impl $trait_name for Vector2 {
            type Output = Self;

            fn $func_name(self, rhs: Self) -> Self::Output {
                Self::new(self.x $op rhs.x, self.y $op rhs.y)
            }
        }
        )*
    };
    (self_assign $($trait_name: ident $func_name: ident $op: tt)*) => {
        $(
        impl $trait_name for Vector2 {
            fn $func_name(&mut self, rhs: Self) {
                self.x $op rhs.x;
                self.y $op rhs.y;
            }
        }
        )*
    };
    (direct_normal $other_ty: ident $($trait_name: ident $func_name: ident $op: tt)*) => {
        $(
        impl $trait_name<$other_ty> for Vector2 {
            type Output = Self;

            fn $func_name(self, rhs: $other_ty) -> Self::Output {
                Self::new(self.x $op rhs, self.y $op rhs)
            }
        }
        impl $trait_name<Vector2> for $other_ty {
            type Output = Vector2;

            fn $func_name(self, rhs: Vector2) -> Self::Output {
                Vector2::new(self $op rhs.x, self $op rhs.y)
            }
        }
        )*
    };
}

#[cfg(feature = "glam")]
use glam::Vec2;

#[cfg(feature = "glam")]
impl From<Vec2> for Vector2 {
    fn from(value: Vec2) -> Self {
        Self::new(value.x, value.y)
    }
}

#[cfg(feature = "glam")]
pub trait AsVector2 {
    fn as_render_vec(self) -> Vector2;
}

#[cfg(feature = "glam")]
impl<T> AsVector2 for T
where
    Vec2: From<T>,
{
    fn as_render_vec(self) -> Vector2 {
        Vec2::from(self).into()
    }
}

#[cfg(feature = "glam")]
impl From<Vector2> for Vec2 {
    fn from(value: Vector2) -> Self {
        Self::new(value.x, value.y)
    }
}

use std::ops::{Add, Div, Mul, Sub};
vec2_op_impl! {
    self_normal
    Add add +
    Sub sub -
    Mul mul *
    Div div /
}

vec2_op_impl! {
    direct_normal f32
    Mul mul *
    Div div /
    Add add +
    Sub sub -
}

use std::ops::{AddAssign, DivAssign, MulAssign, SubAssign};

vec2_op_impl! {
    self_assign
    AddAssign add_assign +=
    SubAssign sub_assign -=
    MulAssign mul_assign *=
    DivAssign div_assign /=
}
