use super::data::Value;
use arrayvec::ArrayVec;
use std::slice::from_raw_parts_mut;

#[derive(Debug, Clone, Copy)]
pub enum Operation {
  AddSelfU8OtherU8OutU8,
  AddAssignSelfU8OtherU8,
}

impl Operation {
  pub fn do_op(self, operand0: &mut Value, operand1: &mut Value, operand2: Option<&mut Value>) {
    match self {
      Operation::AddSelfU8OtherU8OutU8 => {
        *operand2.unwrap().as_u8_mut() = *operand0.as_u8() + *operand1.as_u8()
      }
      Operation::AddAssignSelfU8OtherU8 => *operand0.as_u8_mut() += *operand1.as_u8(),
    }
  }
}

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
