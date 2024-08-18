use bytemuck::{Pod, Zeroable};
use std::mem;

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, PartialEq, Zeroable, Pod)]
pub struct RawColor {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, PartialEq)]
pub struct Color {
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
}

macro_rules! color_constants {
    (
        $($name: ident ($r: expr, $g: expr, $b:expr))*
    ) => {
        #[allow(dead_code)]
        impl Color {
            $(
            pub const $name: Self = Self::new($r, $g, $b, 1.0);
            )*
        }
    };
}

// The equivalent of half in sRGB, in linear RGB
const H: f32 = 0.214041140482;
color_constants! {
    BLACK      (0.0, 0.0, 0.0)
    WHITE      (1.0, 1.0, 1.0)
    DARK_GRAY  (0.25, 0.25, 0.25)
    GRAY       (0.5, 0.5, 0.5)
    LIGHT_GRAY (0.75, 0.75, 0.75)

    RED       (1.0, 0.0, 0.0)
    YELLOW    (1.0, 1.0, 0.0)
    GREEN     (0.0, 1.0, 0.0)
    CYAN      (0.0, 1.0, 1.0)
    PURE_BLUE (0.0, 0.0, 1.0)
    MAGENTA   (1.0, 0.0, 1.0)

    ORANGE (1.0, H, 0.0)
    BLUE   (0.0, H, 1.0)
}

impl Color {
    #[inline]
    pub const fn new(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    #[inline]
    pub const fn with_alpha(self, alpha: f32) -> Self {
        Self::new(self.red, self.green, self.blue, alpha)
    }

    #[inline]
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::new(r, g, b, 1.0)
    }

    #[inline]
    pub const fn raw(self) -> RawColor {
        unsafe { mem::transmute(self) }
    }

    #[inline]
    pub fn raw_pre_mult(self) -> RawColor {
        Self::new(
            self.red * self.alpha,
            self.green * self.alpha,
            self.blue * self.alpha,
            self.alpha,
        )
        .raw()
    }
}
