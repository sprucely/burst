use super::data::Value;
use arrayvec::ArrayVec;
use std::slice::from_raw_parts_mut;

#[derive(Debug, Clone, Copy)]
pub enum Operation {
  AddSelfU8OtherU8OutU8,
  AddAssignSelfU8OtherU8,
}

// pub fn DoOp(op: Operation, operands: &mut ArrayVec<Value, 3>) {
//   match op {
//     Operation::AddSelfU8OtherU8OutU8 => {
//       let out = &mut operands[2].as_u8_mut();
//       let slf = &(operands[0].as_u8());
//       let other = &(operands[1].as_u8());

//       **out = **slf + **other
//     }
//     Operation::AddAssignSelfU8OtherU8 => {}
//   }
// }

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
    {
      let (op1, op2, op3) = split_value_mut(&mut operands);
      {
        let op1u8 = op1.as_u8();
        let op2u8 = op2.as_u8();
        let op3u8 = op3.as_u8_mut();

        println!("{:?}, {:?}, {:?}", *op1u8, *op2u8, *op3u8);

        *op3u8 = *op1u8 + *op2u8 * 10;
        assert_eq!(*op3u8, 21);
      }
    }

    assert_eq!(operands[2].bytes, [21, 0, 0, 0, 0, 0, 0, 0]);
  }
}
