//use super::grammar;

#[test]
fn calculator1() {
  assert!(super::grammar::TermParser::new().parse("22").is_ok());
  assert!(super::grammar::TermParser::new().parse("(22)").is_ok());
  assert!(super::grammar::TermParser::new()
    .parse("((((22))))")
    .is_ok());
  assert!(super::grammar::TermParser::new().parse("((22)").is_err());
}
