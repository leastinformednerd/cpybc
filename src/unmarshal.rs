//! This module should implement the unmarshalling of python objects.
//! It is derived from Tools/build/umarshal.py from the python/Cpython repo

use std::rc::Rc;

enum PyObjectUnresolved {
    Null,
    None,
    Bool(bool),
    StopIter,
    Ellipsis,
    SmallInt(u64),
    LargeInt(PyLargeInt),
    Float(f64),
    Complex(f64, f64),
    String(Rc<str>),
    Ref(u32),
    Tuple(Box<[PyObjectUnresolved]>),
    List(Box<[PyObjectUnresolved]>),
    Dict(Box<[(PyObjectUnresolved, PyObjectUnresolved)]>),
    Set(Box<[PyObjectUnresolved]>),
    Code(CodeObject),
}
type PyLargeInt = Box<[u8]>;

#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum PyTypeTag {
    Null = b'0',
    None = b'N',
    True = b'T',
    False = b'F',
    StopIter = b'S',
    Ellipsis = b'.',
    Int = b'i',
    Int64 = b'I',
    Float = b'f',
    BinaryFloat = b'g',
    Complex = b'x',
    BinaryComplex = b'y',
    Long = b'l',
    String = b's',
    Interned = b't',
    Ref = b'r',
    Tuple = b'(',
    List = b'[',
    Dict = b'{',
    Code = b'c',
    Unicode = b'u',
    Unknown = b'?',
    Set = b'<',
    FrozenSet = b'>',
    Ascii = b'a',
    AsciiInterned = b'A',
    SmallTuple = b')',
    ShortAscii = b'z',
    ShortAsciiInterned = b'Z',
}

impl TryFrom<u8> for PyTypeTag {
    type Error = UnmarshalError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use PyTypeTag::*;
        Ok(match value {
            b'0' => Null,
            b'N' => None,
            b'T' => True,
            b'F' => False,
            b'S' => StopIter,
            b'.' => Ellipsis,
            b'i' => Int,
            b'I' => Int64,
            b'f' => Float,
            b'g' => BinaryFloat,
            b'x' => Complex,
            b'y' => BinaryComplex,
            b'l' => Long,
            b's' => String,
            b't' => Interned,
            b'r' => Ref,
            b'(' => Tuple,
            b'[' => List,
            b'{' => Dict,
            b'c' => Code,
            b'u' => Unicode,
            b'?' => Unknown,
            b'<' => Set,
            b'>' => FrozenSet,
            b'a' => Ascii,
            b'A' => AsciiInterned,
            b')' => SmallTuple,
            b'z' => ShortAscii,
            b'Z' => ShortAsciiInterned,
            _ => return Err(UnmarshalError::InvalidTag),
        })
    }
}

pub struct PyObjectParse {}

#[derive(PartialEq, Eq, Debug)]
pub enum UnmarshalError {
    UnexpectedEof,
    InvalidTag,
}

#[derive(Debug)]
pub struct Unmarshaller<'a> {
    src: &'a [u8],
    objects: Vec<PyObjectUnresolved>,
    refables: Vec<usize>,
}

impl<'a> Unmarshaller<'a> {
    fn loads(src: &'a [u8]) -> Result<PyObjectParse, UnmarshalError> {
        let mut this = Unmarshaller {
            src,
            objects: Vec::new(),
            refables: Vec::new(),
        };
        this.parse_object()
    }

    fn get_byte(&mut self) -> Result<u8, UnmarshalError> {
        let [b, src @ ..] = self.src else {
            return Err(UnmarshalError::UnexpectedEof);
        };
        self.src = src;
        return Ok(*b);
    }

    fn parse_object(&mut self) -> Result<PyObjectUnresolved, UnmarshalError> {
        use PyTypeTag as PT;
        use PyObjectUnresolved as PO
        let r#type = self.get_byte().and_then(TryInto::try_into)?;
        match r#type {
            PT::Null => Ok(PO::Null),
            PT::None => Ok(PO::None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn py_type_tag_conv_iso() {
        fn check_tag(tag: PyTypeTag) {
            assert_eq!((tag as u8).try_into(), Ok(tag), "{tag:?}");
        }
        check_tag(PyTypeTag::Null);
        check_tag(PyTypeTag::None);
        check_tag(PyTypeTag::True);
        check_tag(PyTypeTag::False);
        check_tag(PyTypeTag::StopIter);
        check_tag(PyTypeTag::Ellipsis);
        check_tag(PyTypeTag::Int);
        check_tag(PyTypeTag::Int64);
        check_tag(PyTypeTag::Float);
        check_tag(PyTypeTag::BinaryFloat);
        check_tag(PyTypeTag::Complex);
        check_tag(PyTypeTag::BinaryComplex);
        check_tag(PyTypeTag::Long);
        check_tag(PyTypeTag::String);
        check_tag(PyTypeTag::Interned);
        check_tag(PyTypeTag::Ref);
        check_tag(PyTypeTag::Tuple);
        check_tag(PyTypeTag::List);
        check_tag(PyTypeTag::Dict);
        check_tag(PyTypeTag::Code);
        check_tag(PyTypeTag::Unicode);
        check_tag(PyTypeTag::Unknown);
        check_tag(PyTypeTag::Set);
        check_tag(PyTypeTag::FrozenSet);
        check_tag(PyTypeTag::Ascii);
        check_tag(PyTypeTag::AsciiInterned);
        check_tag(PyTypeTag::SmallTuple);
        check_tag(PyTypeTag::ShortAscii);
        check_tag(PyTypeTag::ShortAsciiInterned);
    }
}
