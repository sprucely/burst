use core::cmp::Ordering;
use paste::paste;

macro_rules! val_as {
  ($($type_name:ty)+) => {
    paste! {
      $(
        #[inline(always)]
        pub fn [<as_ $type_name:snake>](&self) -> &$type_name {
          let (_head, body, _tail) = unsafe { self.bytes.align_to::<$type_name>() };
          return &body[0];
        }

        #[inline(always)]
        pub fn [<as_ $type_name:snake _mut>](&mut self) -> &mut $type_name {
          let (_head, body, _tail) = unsafe { self.bytes.align_to_mut::<$type_name>() };
          return &mut body[0];
        }
      )*
    }
  };
}

#[derive(Debug, Clone, Copy)]
pub struct Value {
  pub bytes: [u8; 8],
}

impl PartialEq for Value {
  fn eq(&self, other: &Value) -> bool {
    self.bytes == other.bytes
  }
}

impl PartialOrd for Value {
  #[inline]
  fn partial_cmp(&self, other: &Value) -> Option<Ordering> {
    PartialOrd::partial_cmp(&self.bytes, &other.bytes)
  }
}

// array types are not accepted as macro type/ty arguments, so give them an alias...
pub type U16X4 = [u16; 4];
pub type U32X2 = [u32; 2];
pub type I16X4 = [i16; 4];
pub type I32X2 = [i32; 2];
pub type F32X2 = [f32; 2];

impl Value {
  val_as!(u8 u16 u32 u64 i8 i16 i32 i64 f32 f64 U16X4 U32X2 I16X4 I32X2 F32X2);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_val_as_u8() {
    let mut val = Value {
      bytes: [254, 0, 0, 0, 0, 0, 0, 0],
    };
    {
      let a = val.as_u8_mut();
      *a += 1;
    }
    let b = val.as_u8();
    assert_eq!(*b, 255);
    assert_eq!(val.bytes, [255, 0, 0, 0, 0, 0, 0, 0]);
  }

  #[test]
  fn test_val_as_u32_x2() {
    let mut val = Value {
      bytes: [254, 0, 0, 0, 254, 0, 0, 0],
    };
    {
      let a = val.as_u32_x2_mut();
      a[1] += 1;
    }
    let b = val.as_u32_x2_mut();
    assert_eq!(b[1], 255);
    assert_eq!(val.bytes, [254, 0, 0, 0, 255, 0, 0, 0]);
  }

  #[test]
  fn test_val_as_f32_x2() {
    let mut val = Value {
      bytes: [0, 0, 0, 0, 0, 0, 0, 0],
    };
    let f = 1.5;
    {
      let a = val.as_f32_x2_mut();
      a[1] = f;
    }
    let b = val.as_f32_x2_mut();
    assert_eq!(b[1], f);
  }
}
