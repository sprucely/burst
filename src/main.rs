mod data;

fn main() {
  let mut data = crate::data::Data {
    bytes: [3, 0, 1, 5, 0, 0, 0, 0],
  };
  {
    let a = data.as_u8_mut();
    *a += 1;
  }
  let b = data.as_u8();

  println!("{:?}", data);
  println!("{:?}", b);
}
