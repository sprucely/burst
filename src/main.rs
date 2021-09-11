#![allow(dead_code)]

mod data;
mod ops;

fn main() {
  let mut val = crate::data::Value {
    bytes: [3, 0, 1, 5, 0, 0, 0, 0],
  };
  {
    let a = val.as_u8_mut();
    *a += 1;
  }
  let b = val.as_u8();

  println!("{:?}", val);
  println!("{:?}", b);
}
