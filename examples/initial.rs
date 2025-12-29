use cpybc::{abstract_interpretation::eval::eval314, unmarshal::Unmarshaller};

fn main() {
    let example_pyc = std::fs::read("examples/initial.pyc").unwrap();
    let parse = Unmarshaller::loads(&example_pyc[16..]).unwrap();
    let res = eval314(parse.construct().unwrap());
    println!("{:#?}", res)
}
