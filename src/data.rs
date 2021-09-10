use core::cmp::Ordering;
use paste::paste;

#[macro_export]
macro_rules! data_as {
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
pub struct Data {
  pub bytes: [u8; 8],
}

impl PartialEq for Data {
  fn eq(&self, other: &Data) -> bool {
    self.bytes == other.bytes
  }
}

impl PartialOrd for Data {
  #[inline]
  fn partial_cmp(&self, other: &Data) -> Option<Ordering> {
    PartialOrd::partial_cmp(&self.bytes, &other.bytes)
  }
}

// array types are not accepted as macro type/ty arguments, so give them an alias...
type U16X4 = [u16; 4];
type U32X2 = [u32; 2];
type I16X4 = [i16; 4];
type I32X2 = [i32; 2];
type F32X2 = [f32; 2];

impl Data {
  data_as!(u8 u16 u32 u64 i8 i16 i32 i64 f32 f64 U16X4 U32X2 I16X4 I32X2 F32X2);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_data_as_u8() {
    let mut data = Data {
      bytes: [254, 0, 0, 0, 0, 0, 0, 0],
    };
    {
      let a = data.as_u8_mut();
      *a += 1;
    }
    let b = data.as_u8();
    assert_eq!(*b, 255);
    assert_eq!(data.bytes, [255, 0, 0, 0, 0, 0, 0, 0]);
  }

  #[test]
  fn test_data_as_u32_x2() {
    let mut data = Data {
      bytes: [254, 0, 0, 0, 254, 0, 0, 0],
    };
    {
      let a = data.as_u32_x2_mut();
      a[1] += 1;
    }
    let b = data.as_u32_x2_mut();
    assert_eq!(b[1], 255);
    assert_eq!(data.bytes, [254, 0, 0, 0, 255, 0, 0, 0]);
  }

  #[test]
  fn test_data_as_f32_x2() {
    let mut data = Data {
      bytes: [0, 0, 0, 0, 0, 0, 0, 0],
    };
    let f = 1.5;
    {
      let a = data.as_f32_x2_mut();
      a[1] = f;
    }
    let b = data.as_f32_x2_mut();
    assert_eq!(b[1], f);
  }
}
