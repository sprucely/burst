use super::data::Value;
use arrayvec::ArrayVec;
use paste::paste;
use std::slice::from_raw_parts_mut;

macro_rules! define_match {
  ($self:ident, $op0:ident, $op1:ident, $op2:ident $($func:ident($op:tt $num:ident ($($type_name:tt)+)))+) => {
      define_match!(@ $self, $op0, $op1, $op2 {[]} $($func($op $num ($($type_name)+)))+);
  };
  (@ $self:ident, $op0:ident, $op1:ident, $op2:ident {[$($match:tt)*]} $func:ident($op:tt $num:ident ($($type_name:tt)+)) $($tail:tt)*) => {
    define_match!(@ $self, $op0, $op1, $op2 {[$($match)*]} $($func($op $num $type_name))+ $($tail)*);
  };
  (@ $self:ident, $op0:ident, $op1:ident, $op2:ident {[$($match:tt)*]} $func:ident($op:tt two $type_name:ty) $($tail:tt)*) => {
    paste! {
      define_match!(@ $self, $op0, $op1, $op2 {
        [$($match)* Operation::[<$func Self $type_name:upper Other $type_name:upper>] => *$op0.[<as_ $type_name _mut>]() $op *$op1.[<as_ $type_name>](),]
      } $($tail)*);
    }
  };
  (@ $self:ident, $op0:ident, $op1:ident, $op2:ident {[$($match:tt)*]} $func:ident($op:tt three $type_name:ty) $($tail:tt)*) => {
    paste! {
      define_match!(@ $self, $op0, $op1, $op2 {
        [$($match)* Operation::[<$func Self $type_name:upper Other $type_name:upper Out $type_name:upper>] => *$op2.unwrap().[<as_ $type_name _mut>]() = *$op0.[<as_ $type_name>]() $op *$op1.[<as_ $type_name>](),]
      } $($tail)*);
    }
  };
  (@ $self:ident, $op0:ident, $op1:ident, $op2:ident {[$($match:tt)*]}) => {
        match $self {
          $($match)*
        }
  };
}

macro_rules! define_ops {
  ($($func:ident($op:tt $num:ident ($($type_name:tt)+)))+) => {
      define_ops!(@ {[]} $($func($op $num ($($type_name)+)))+);

      impl Operation {
        pub fn do_op(self, operand0: &mut Value, operand1: &mut Value, operand2: Option<&mut Value>) {
          // variables must be passed in for hygienic purposes
          define_match! (self, operand0, operand1, operand2
            $($func($op $num ($($type_name)+)))+
          );
        }
      }

    };

  (@ {[$($variant:tt)*]} $func:ident($op:tt $num:ident ($($type_name:tt)+)) $($tail:tt)*) => {
    define_ops!(@ {[$($variant)*]} $($func($op $num $type_name))+ $($tail)*);
  };

  (@ {[$($variant:tt)*]} $func:ident($op:tt two $type_name:ty) $($tail:tt)*) => {
    paste! {
      define_ops!(@ {
        [$($variant)* [<$func Self $type_name:upper Other $type_name:upper>],]
      } $($tail)*);
    }
  };

  (@ {[$($variant:tt)*]} $func:ident($op:tt three $type_name:ty) $($tail:tt)*) => {
    paste! {
      define_ops!(@ {
        [$($variant)* [<$func Self $type_name:upper Other $type_name:upper Out $type_name:upper>],]
      } $($tail)*);
    }
  };

  (@ {[$($variant:tt)*]}) => {
    #[derive(Debug, Clone, Copy)]
    pub enum Operation {
      //$(println!(stringify!($variant));)*
      $($variant)*
    }
  };
}

// An example of what the folowing define_ops!(...) generates
// #[derive(Debug, Clone, Copy)]
// pub enum Operation {
//   AddSelfU8OtherU8OutU8,
//   AddAssignSelfU8OtherU8,
// }

// impl Operation {
//   pub fn do_op(self, operand0: &mut Value, operand1: &mut Value, operand2: Option<&mut Value>) {
//     match self {
//       Operation::AddSelfU8OtherU8OutU8 => {
//         *operand2.unwrap().as_u8_mut() = *operand0.as_u8() + *operand1.as_u8()
//       }
//       Operation::AddAssignSelfU8OtherU8 => *operand0.as_u8_mut() += *operand1.as_u8(),
//       _ => panic!(),
//     }
//   }
// }
define_ops! (
  Add(+ three (u8 u16 u32 u64 i8 i16 i32 i64 f32 f64))
  AddAssign(+= two (u8 u16 u32 u64 i8 i16 i32 i64 f32 f64))
  BitAnd(& three (u8 u16 u32 u64 i8 i16 i32 i64))
  BitAndAssign(&= two (u8 u16 u32 u64 i8 i16 i32 i64))
  BitOr(| three (u8 u16 u32 u64 i8 i16 i32 i64))
  BitOrAssign(|= two (u8 u16 u32 u64 i8 i16 i32 i64))
  BitXor(^ three (u8 u16 u32 u64 i8 i16 i32 i64))
  BitXorAssign(^= two (u8 u16 u32 u64 i8 i16 i32 i64))
  Div(/ three (u8 u16 u32 u64 i8 i16 i32 i64 f32 f64))
  DivAssign(/= two (u8 u16 u32 u64 i8 i16 i32 i64 f32 f64))
  Mul(* three (u8 u16 u32 u64 i8 i16 i32 i64 f32 f64))
  MulAssign(*= two (u8 u16 u32 u64 i8 i16 i32 i64 f32 f64))
  Rem(% three (u8 u16 u32 u64 i8 i16 i32 i64 f32 f64))
  RemAssign(%= two (u8 u16 u32 u64 i8 i16 i32 i64 f32 f64))
  Shl(<< three (u8 u16 u32 u64 i8 i16 i32 i64))
  ShlAssign(<<= two (u8 u16 u32 u64 i8 i16 i32 i64))
  Shr(>> three (u8 u16 u32 u64 i8 i16 i32 i64))
  ShrAssign(>>= two (u8 u16 u32 u64 i8 i16 i32 i64))
  Sub(- three (u8 u16 u32 u64 i8 i16 i32 i64 f32 f64))
  SubAssign(-= two (u8 u16 u32 u64 i8 i16 i32 i64 f32 f64))
);
// TODO: Figure out what, if anything, to do with the following ops...
// Neg
// Not
// Index
// IndexMut
// RangeBounds

type ValueX3 = ArrayVec<Value, 3>;

pub fn split_value_mut(values: &mut ValueX3) -> (&mut Value, &mut Value, &mut Value) {
  let ptr = values.as_mut_ptr();

  unsafe {
    (
      &mut from_raw_parts_mut(ptr, 8)[0],
      &mut from_raw_parts_mut(ptr.add(1), 8)[0],
      &mut from_raw_parts_mut(ptr.add(2), 8)[0],
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_do_op() {
    let mut operand0 = Value {
      bytes: [1, 0, 0, 0, 0, 0, 0, 0],
    };
    let mut operand1 = Value {
      bytes: [2, 0, 0, 0, 0, 0, 0, 0],
    };
    let mut operand2 = Value {
      bytes: [3, 0, 0, 0, 0, 0, 0, 0],
    };

    Operation::AddAssignSelfU8OtherU8.do_op(&mut operand0, &mut operand1, None);
    assert_eq!(operand0.bytes, [3, 0, 0, 0, 0, 0, 0, 0]);

    Operation::AddSelfU8OtherU8OutU8.do_op(&mut operand0, &mut operand1, Some(&mut operand2));
    assert_eq!(operand2.bytes, [5, 0, 0, 0, 0, 0, 0, 0]);
  }

  #[test]
  fn test_split_value_mut() {
    let mut operands = ValueX3::new();
    operands.push(Value {
      bytes: [1, 0, 0, 0, 0, 0, 0, 0],
    });
    operands.push(Value {
      bytes: [2, 0, 0, 0, 0, 0, 0, 0],
    });
    operands.push(Value {
      bytes: [3, 0, 0, 0, 0, 0, 0, 0],
    });
    let (op1, op2, op3) = split_value_mut(&mut operands);
    {
      *op3.as_u8_mut() = *op1.as_u8() + *op2.as_u8() * 10;
    }

    assert_eq!(operands[2].bytes, [21, 0, 0, 0, 0, 0, 0, 0]);
  }
}
