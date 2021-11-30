#![allow(dead_code)]
#![recursion_limit = "512"]

#[macro_use]
extern crate lalrpop_util;
lalrpop_mod!(pub grammar); // synthesized by LALRPOP

mod component;
mod component_instance;
mod orchestrator;
// mod data;
// mod ops;
mod parser;

fn main() {
  /*  let mut val = crate::data::Value {
    bytes: [3, 0, 1, 5, 0, 0, 0, 0],
  };
  {
    let a = val.as_u8_mut();
    *a += 1;
  }
  let b = val.as_u8();

  println!("{:?}", val);
  println!("{:?}", b);*/
}
