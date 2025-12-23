use cpybc::{
    abstract_interpretation::eval::eval314,
    objects::{CodeObject, CodeObjectConstructor, PyObject},
    unmarshal::Unmarshaller,
};

fn main() {
    let example_pyc = std::fs::read("examples/initial.pyc").unwrap();
    let parse = Unmarshaller::loads(&example_pyc[16..]).unwrap();
    let Some(PyObject::Code(co)) = parse.first() else {
        panic!("Expected the root of the parse to be a code object")
    };
    let input = co.construct(&parse).unwrap();
    println!("{:#?}", eval314(input, &parse))
}
