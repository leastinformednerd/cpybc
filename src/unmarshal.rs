//! This module should implement the unmarshalling of python objects.
//! It is derived from Tools/build/umarshal.py from the python/Cpython repo

use std::collections::BTreeSet;

#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct PyObjectIndex(usize);

#[derive(Debug, PartialEq)]
enum PyObject {
    Null,
    None,
    Bool(bool),
    StopIter,
    Ellipsis,
    SmallInt(i64),
    LargeInt(PyLargeInt),
    Float(f64),
    Complex(f64, f64),
    Bytes(Box<[u8]>),
    String(Box<str>),
    // These usize indices are here because I'm unclear on the ownership of
    // the objects stored in these collections.
    Tuple(Box<[PyObjectIndex]>),
    List(Box<[PyObjectIndex]>),
    Dict(Box<[(PyObjectIndex, PyObjectIndex)]>),
    Set(Box<[PyObjectIndex]>),
    FrozenSet(Box<[PyObjectIndex]>),
    Code(CodeObjectConstructor),
}
type PyLargeInt = Box<[u8]>;

#[derive(Debug, PartialEq)]
pub struct CodeObjectConstructor {
    arg_count: i32,
    pos_only_arg_count: i32,
    kw_only_arg_count: i32,
    stack_size: i32,
    flags: i32,
    code: PyObjectIndex,
    consts: PyObjectIndex,
    names: PyObjectIndex,
    // Tuple mapping offsets to names
    locals_plus_names: PyObjectIndex,
    // This is a list that corresponds to free and cell flags on locals
    locals_plus_kinds: PyObjectIndex,
    filename: PyObjectIndex,
    name: PyObjectIndex,
    qualified_name: PyObjectIndex,
    first_line_no: i32,
    line_table: PyObjectIndex,
    exception_table: PyObjectIndex,
}

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

#[derive(PartialEq, Eq, Debug)]
pub enum UnmarshalError {
    UnexpectedEof,
    InvalidTag,
    DecodingError,
    ExplicitUnknown,
    FoundNull,
    DanglingRef(usize),
}

#[derive(Debug)]
pub struct Unmarshaller<'a> {
    src: &'a [u8],
    objects: Vec<PyObject>,
    refables: Vec<usize>,
}

#[derive(Debug, PartialEq)]
pub struct PyObjectRegion(Vec<PyObject>);

impl<'a> Unmarshaller<'a> {
    pub fn loads(src: &'a [u8]) -> Result<PyObjectRegion, UnmarshalError> {
        let mut this = Unmarshaller {
            src,
            objects: Vec::new(),
            refables: Vec::new(),
        };
        let obj = this.parse_object()?;

        assert_eq!(obj.0, 0);

        Ok(PyObjectRegion(this.objects))
    }

    fn get_byte(&mut self) -> Result<u8, UnmarshalError> {
        let [b, src @ ..] = self.src else {
            return Err(UnmarshalError::UnexpectedEof);
        };
        self.src = src;
        Ok(*b)
    }

    fn get_bytes<const N: usize>(&mut self) -> Result<[u8; N], UnmarshalError> {
        let Some((b, rest)) = self.src.split_first_chunk() else {
            return Err(UnmarshalError::UnexpectedEof);
        };

        self.src = rest;
        Ok(*b)
    }

    fn get_short_str(&mut self) -> Result<&[u8], UnmarshalError> {
        let len = self.get_byte()?;
        let Some(s) = self.src.split_off(..(len as usize)) else {
            return Err(UnmarshalError::UnexpectedEof);
        };
        Ok(s)
    }

    fn get_str(&mut self) -> Result<&[u8], UnmarshalError> {
        let len = u32::from_le_bytes(self.get_bytes()?);
        let Some(s) = self.src.split_off(..(len as usize)) else {
            return Err(UnmarshalError::UnexpectedEof);
        };
        Ok(s)
    }

    const FLAG: u8 = 0x80;
    fn parse_object(&mut self) -> Result<PyObjectIndex, UnmarshalError> {
        use PyObject as PO;
        use PyTypeTag as PT;
        let tag = self.get_byte()?;

        let flag = tag & Self::FLAG != 0;

        let r#type = (tag & !Self::FLAG).try_into()?;
        let parse = match r#type {
            PT::Null => return Err(UnmarshalError::FoundNull),
            PT::None => PO::None,
            PT::True => PO::Bool(true),
            PT::False => PO::Bool(false),
            PT::StopIter => PO::StopIter,
            PT::Ellipsis => PO::Ellipsis,
            PT::Int => PyObject::SmallInt(i32::from_le_bytes(self.get_bytes()?).into()),
            PT::Int64 => PyObject::SmallInt(i64::from_le_bytes(self.get_bytes()?).into()),
            PT::Float => self.parse_fstr()?,
            PT::BinaryFloat => PyObject::Float(f64::from_le_bytes(self.get_bytes()?).into()),
            PT::Complex => self.parse_cstr()?,
            PT::BinaryComplex => PyObject::Complex(
                f64::from_le_bytes(self.get_bytes()?).into(),
                f64::from_le_bytes(self.get_bytes()?).into(),
            ),
            PT::Long => PO::LargeInt(self.get_str()?.into()),
            PT::String => PO::Bytes(self.get_str()?.into()),
            PT::Interned | PT::Unicode => self.parse_str()?,
            PT::Ref => {
                let ref_idx = u32::from_le_bytes(self.get_bytes()?) as usize;
                return match self.refables.get(ref_idx) {
                    Some(idx) => Ok(PyObjectIndex(*idx)),
                    None => Err(UnmarshalError::DanglingRef(ref_idx)),
                };
            }
            PT::Tuple => return self.parse_sequence(flag, PO::Tuple),
            PT::List => return self.parse_sequence(flag, PO::List),
            // TODO: Make these implementations not awful
            PT::Set => {
                return self.parse_sequence(flag, |b| {
                    PO::Set(
                        b.into_iter()
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect::<Vec<_>>()
                            .into(),
                    )
                });
            }
            PT::FrozenSet => {
                return self.parse_sequence(flag, |b| {
                    PO::FrozenSet(
                        b.into_iter()
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect::<Vec<_>>()
                            .into(),
                    )
                });
            }
            PT::SmallTuple => {
                let n = self.get_byte()?;
                let idx = self.objects.len();
                if flag {
                    self.refables.push(idx);
                }
                self.objects.push(PO::Null);
                let mut v = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    v.push(self.parse_object()?);
                }
                self.objects[idx] = PO::Tuple(v.into_boxed_slice());
                return Ok(PyObjectIndex(idx));
            }
            PT::Dict => return self.parse_dict(flag),
            PT::Code => return self.parse_code(flag),
            PT::Ascii | PT::AsciiInterned => {
                let bytes = self.get_str()?;
                match str::from_utf8(bytes) {
                    Ok(s) => PO::String(s.into()),
                    Err(_) => return Err(UnmarshalError::DecodingError),
                }
            }
            PT::ShortAscii | PT::ShortAsciiInterned => {
                let bytes = self.get_short_str()?;
                match str::from_utf8(bytes) {
                    Ok(s) => PO::String(s.into()),
                    Err(_) => return Err(UnmarshalError::DecodingError),
                }
            }
            PT::Unknown => return Err(UnmarshalError::ExplicitUnknown),
        };

        let idx = self.objects.len();
        self.objects.push(parse);
        if flag {
            self.refables.push(idx);
        }
        Ok(PyObjectIndex(idx))
    }

    fn parse_str(&mut self) -> Result<PyObject, UnmarshalError> {
        let s = self.get_str()?;
        match str::from_utf8(s) {
            Ok(s) => Ok(PyObject::String(s.into())),
            Err(_) => Err(UnmarshalError::DecodingError),
        }
    }

    fn parse_sequence(
        &mut self,
        flag: bool,
        constructor: fn(Box<[PyObjectIndex]>) -> PyObject,
    ) -> Result<PyObjectIndex, UnmarshalError> {
        let idx = self.objects.len();
        self.objects.push(PyObject::Null);
        if flag {
            self.refables.push(idx);
        };
        let len = i32::from_le_bytes(self.get_bytes()?);
        if len < 0 {
            return Err(UnmarshalError::DecodingError);
        }
        let obj = constructor(self.parse_list(len as usize)?);
        self.objects[idx] = obj;
        return Ok(PyObjectIndex(idx));
    }

    fn parse_dict(&mut self, flag: bool) -> Result<PyObjectIndex, UnmarshalError> {
        let idx = self.objects.len();
        if flag {
            self.refables.push(idx);
        };
        self.objects.push(PyObject::Null);
        // I'm assuming that 10 is probably a sensible default for capacity
        let mut d = Vec::with_capacity(10);
        loop {
            let key = match self.parse_object() {
                Ok(key) => key,
                Err(UnmarshalError::FoundNull) => break,
                err => return err,
            };
            let value = self.parse_object()?;
            d.push((key, value))
        }
        let obj = PyObject::Dict(d.into_boxed_slice());
        self.objects[idx] = obj;
        return Ok(PyObjectIndex(idx));
    }

    fn parse_code(&mut self, flag: bool) -> Result<PyObjectIndex, UnmarshalError> {
        let idx = self.objects.len();
        if flag {
            self.refables.push(idx);
        }
        self.objects.push(PyObject::Null);
        let obj = CodeObjectConstructor {
            arg_count: i32::from_le_bytes(self.get_bytes()?),
            pos_only_arg_count: i32::from_le_bytes(self.get_bytes()?),
            kw_only_arg_count: i32::from_le_bytes(self.get_bytes()?),
            stack_size: i32::from_le_bytes(self.get_bytes()?),
            flags: i32::from_le_bytes(self.get_bytes()?),
            code: self.parse_object()?,
            consts: self.parse_object()?,
            names: self.parse_object()?,
            locals_plus_names: self.parse_object()?,
            locals_plus_kinds: self.parse_object()?,
            filename: self.parse_object()?,
            name: self.parse_object()?,
            qualified_name: self.parse_object()?,
            first_line_no: i32::from_le_bytes(self.get_bytes()?),
            line_table: self.parse_object()?,
            exception_table: self.parse_object()?,
        };
        self.objects[idx] = PyObject::Code(obj);
        Ok(PyObjectIndex(idx))
    }

    fn parse_cstr(&mut self) -> Result<PyObject, UnmarshalError> {
        let Ok(s1) = str::from_utf8(self.get_short_str()?) else {
            return Err(UnmarshalError::DecodingError);
        };
        let f1 = s1.parse();

        let Ok(s2) = str::from_utf8(self.get_short_str()?) else {
            return Err(UnmarshalError::DecodingError);
        };
        let f2 = s2.parse();

        match (f1, f2) {
            (Ok(f1), Ok(f2)) => Ok(PyObject::Complex(f1, f2)),
            _ => Err(UnmarshalError::DecodingError),
        }
    }

    fn parse_fstr(&mut self) -> Result<PyObject, UnmarshalError> {
        let Ok(s) = str::from_utf8(self.get_short_str()?) else {
            return Err(UnmarshalError::DecodingError);
        };

        match s.parse::<f64>() {
            Ok(f) => Ok(PyObject::Float(f)),
            Err(_) => Err(UnmarshalError::DecodingError),
        }
    }

    fn parse_list(&mut self, len: usize) -> Result<Box<[PyObjectIndex]>, UnmarshalError> {
        let mut v = Vec::with_capacity(len);
        for _ in 0..len {
            let idx = self.parse_object()?;
            v.push(idx);
        }
        Ok(v.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    /// This is a test that the tags can be converted losslessly between u8 and
    /// the explicit enum
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

    #[test]
    fn unmarshal_null() {
        let res = Unmarshaller::loads(b"0");
        assert_eq!(Err(UnmarshalError::FoundNull), res);
    }

    #[test]
    fn unmarshal_none() {
        let res = Unmarshaller::loads(b"N");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling None failed");
        };
        assert_eq!(
            objects.as_slice(),
            &[PyObject::None],
            "Incorrectly unmarshalled None"
        )
    }

    #[test]
    fn unmarshal_false() {
        let res = Unmarshaller::loads(b"F");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling false failed");
        };

        assert_eq!(
            objects.as_slice(),
            &[PyObject::Bool(false)],
            "Incorrectly unmarshalled false"
        )
    }

    #[test]
    fn unmarshal_true() {
        let res = Unmarshaller::loads(b"T");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling true failed");
        };

        assert_eq!(
            objects.as_slice(),
            &[PyObject::Bool(true)],
            "Incorrectly unmarshalled true"
        )
    }

    #[test]
    fn unmarshal_stop_iter() {
        let res = Unmarshaller::loads(b"S");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling StopIteration failed");
        };

        assert_eq!(
            objects.as_slice(),
            &[PyObject::StopIter],
            "Incorrectly unmarshalled StopIteration"
        )
    }

    #[test]
    fn unmarshal_ellipsis() {
        let res = Unmarshaller::loads(b".");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling Ellipsis failed");
        };

        assert_eq!(
            objects.as_slice(),
            &[PyObject::Ellipsis],
            "Incorrectly unmarshalled Ellipsis"
        )
    }

    #[test]
    fn unmarshal_pos_small_int() {
        let res = Unmarshaller::loads(&[b'i', 1, 1, 0, 0]);
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling 257i32 failed");
        };

        assert_eq!(
            objects.as_slice(),
            &[PyObject::SmallInt(257)],
            "Incorrectly unmarshalled 257i32"
        )
    }

    #[test]
    fn unmarshal_neg_small_int() {
        let res = Unmarshaller::loads(&[b'i', 0xff, 0xfe, 0xff, 0xff]);
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling -257i32 failed");
        };

        assert_eq!(
            objects.as_slice(),
            &[PyObject::SmallInt(-257)],
            "Incorrectly unmarshalled -257i32"
        )
    }

    #[test]
    fn unmarshal_pos_int64() {
        let res = Unmarshaller::loads(&[b'I', 1, 1, 0, 0, 0, 0, 0, 0]);
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling 257i64 failed");
        };

        assert_eq!(
            objects.as_slice(),
            &[PyObject::SmallInt(257)],
            "Incorrectly unmarshalled 257i64"
        )
    }

    #[test]
    fn unmarshal_neg_int64() {
        let res = Unmarshaller::loads(&[b'I', 0xff, 0xfe, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling -257i64 failed");
        };

        assert_eq!(
            objects.as_slice(),
            &[PyObject::SmallInt(-257)],
            "Incorrectly unmarshalled -257i64"
        )
    }

    #[test]
    fn unmarshal_invalid_int64() {
        let res = Unmarshaller::loads(b"Iabcdef");
        assert_eq!(
            Err(UnmarshalError::UnexpectedEof),
            res,
            "Expected unmarshalling an int64 with less than 8 bytes to fail with EOF"
        );
    }

    #[test]
    fn unmarshal_pos_binary_float() {
        let res = Unmarshaller::loads(&[b'g', 0, 0, 0, 0, 0, 0x10, 0x70, 0x40]);
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling 257f64 failed");
        };

        assert_eq!(
            objects.as_slice(),
            &[PyObject::Float(257.0)],
            "Incorrectly unmarshalled 257f64"
        )
    }

    #[test]
    fn unmarshal_neg_binary_float() {
        let res = Unmarshaller::loads(&[b'g', 0, 0, 0, 0, 0, 0x10, 0x70, 0xc0]);
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling -257f64 failed");
        };

        assert_eq!(
            objects.as_slice(),
            &[PyObject::Float(-257.0)],
            "Incorrectly unmarshalled -257f64"
        )
    }

    #[test]
    fn unmarshal_invalid_binary_float() {
        let res = Unmarshaller::loads(b"gabcdef");
        assert_eq!(
            Err(UnmarshalError::UnexpectedEof),
            res,
            "Expected unmarshalling a float64 with less than 8 bytes to fail with EOF"
        );
    }

    #[test]
    fn unmarshal_pos_str_float() {
        let res = Unmarshaller::loads(b"f\x04257.");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling 257f64 from string form failed");
        };

        assert_eq!(
            objects.as_slice(),
            &[PyObject::Float(257.0)],
            "Incorrectly unmarshalled 257f64 (string form)"
        )
    }

    #[test]
    fn unmarshal_neg_str_float() {
        let res = Unmarshaller::loads(b"f\x06-257.0");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling -257f64 from string form failed");
        };

        assert_eq!(
            objects.as_slice(),
            &[PyObject::Float(-257.0)],
            "Incorrectly unmarshalled -257f64 (string form)"
        )
    }

    #[test]
    fn unmarshal_invalid_str_float() {
        let res = Unmarshaller::loads(b"f\x10abc");
        assert_eq!(
            Err(UnmarshalError::UnexpectedEof),
            res,
            "Expected unmarshalling a str float with insufficient data for string"
        );
    }

    #[test]
    fn unmarshal_binary_complex() {
        let res = Unmarshaller::loads(&[
            b'y', 0, 0, 0, 0, 0, 0x10, 0x70, 0x40, 0, 0, 0, 0, 0, 0x10, 0x70, 0xc0,
        ]);
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling 257-257j failed");
        };

        assert_eq!(
            objects.as_slice(),
            &[PyObject::Complex(257.0, -257.0)],
            "Incorrectly unmarshalled 257-257"
        )
    }

    #[test]
    fn unmarshal_str_complex() {
        let res = Unmarshaller::loads(b"x\x03257\x05-257.");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling 257-257ji from string failed");
        };

        assert_eq!(
            objects.as_slice(),
            &[PyObject::Complex(257.0, -257.0)],
            "Incorrectly unmarshalled 257-257 (from string)"
        )
    }

    #[test]
    fn barebones_unmarshal_long() {
        let res = Unmarshaller::loads(&[b'l', 2, 0, 0, 0, 0, 1]);
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling long [0,1] from string failed, {res:?}");
        };

        assert_eq!(
            objects.as_slice(),
            &[PyObject::LargeInt(Box::new([0, 1]))],
            "Incorrectly unmarshalled long [0,1]"
        )
    }

    #[test]
    fn unmarshal_bytes() {
        let res = Unmarshaller::loads(&[b's', 3, 0, 0, 0, 0, 1, 1]);
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling bytes([0,1,1]) from string failed, {res:?}");
        };

        assert_eq!(
            objects.as_slice(),
            &[PyObject::Bytes(Box::new([0, 1, 1]))],
            "Incorrectly unmarshalled bytes([0,1,1])"
        )
    }

    #[test]
    fn unmarshal_bytes_eof() {
        let res = Unmarshaller::loads(&[b's', 3, 0, 0, 0, 0, 1]);
        assert_eq!(
            res,
            Err(UnmarshalError::UnexpectedEof),
            "Expected unmarshalling a bytes object with not enough bytes to be EOF"
        );
    }

    #[test]
    fn unmarshal_unicode_string() {
        let resu = Unmarshaller::loads(b"u\x03\x00\x00\x00abc");
        let resi = Unmarshaller::loads(b"t\x03\x00\x00\x00abc");
        assert_eq!(
            resu, resi,
            "Uncidode unmarshalling {resu:?} should equal intern unmarshalling {resi:?}"
        );

        let Ok(PyObjectRegion(objects)) = resu else {
            panic!("Unmarshalling \"abc\"failed, {resu:?}");
        };

        assert_eq!(
            objects.as_slice(),
            &[PyObject::String("abc".into())],
            "Incorrectly unmarshalled \"abc\""
        )
    }

    #[test]
    fn unmarshal_unicode_string_eof() {
        let resu = Unmarshaller::loads(b"u\x10\x00\x00\x00bla");
        let resi = Unmarshaller::loads(b"t\x10\x00\x00\x00bla");
        assert_eq!(
            resu, resi,
            "Uncidode unmarshalling {resu:?} should equal intern unmarshalling {resi:?}"
        );

        assert_eq!(
            resu,
            Err(UnmarshalError::UnexpectedEof),
            "Expected eof while parsing \"bla\" as a 0x10 byte long string"
        );
    }

    #[test]
    fn unmarshal_tuple() {
        let res = Unmarshaller::loads(b"(\x02\x00\x00\x00i\x01\x01\x00\x00i\x00\x00\x01\x01");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling (257, 16842752) failed, {res:?}");
        };

        assert_eq!(
            objects.as_slice(),
            &[
                PyObject::Tuple(Box::new([PyObjectIndex(1), PyObjectIndex(2)])),
                PyObject::SmallInt(257),
                PyObject::SmallInt(16842752)
            ],
            "Incorrectly unmarshalled (257, 16842752)"
        );
    }

    #[test]
    fn unmarshal_tuple_eof() {
        let res = Unmarshaller::loads(b"(\x10\x00\x00\x00NNN");

        assert_eq!(
            res,
            Err(UnmarshalError::UnexpectedEof),
            "Expected eof while parsing (None, None, None) as a 0x10 item tuple"
        );
    }

    #[test]
    fn unmarshal_small_tuple() {
        let res = Unmarshaller::loads(b")\x02i\x01\x01\x00\x00i\x00\x00\x01\x01");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling short tuple (257, 16842752) failed, {res:?}");
        };

        assert_eq!(
            objects.as_slice(),
            &[
                PyObject::Tuple(Box::new([PyObjectIndex(1), PyObjectIndex(2)])),
                PyObject::SmallInt(257),
                PyObject::SmallInt(16842752)
            ],
            "Incorrectly unmarshalled short tuple (257, 16842752)"
        );
    }

    #[test]
    fn unmarshal_small_tuple_eof() {
        let res = Unmarshaller::loads(b")\x10NNN");

        assert_eq!(
            res,
            Err(UnmarshalError::UnexpectedEof),
            "Expected eof while parsing (None, None, None) as a 0x10 item small tuple"
        );
    }

    #[test]
    /// Tests unmarshalling a tuple where one element is a reference to the other
    /// The input bytestring is directly from marshal.dumps((1,1))
    fn unmarshal_tuple_with_self_reference() {
        let res = Unmarshaller::loads(b"\xa9\x02\xe9\x01\x00\x00\x00r\x01\x00\x00\x00");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling short tuple (1, 1) failed, {res:?}");
        };
        assert_eq!(
            objects.as_slice(),
            &[
                PyObject::Tuple(Box::new([PyObjectIndex(1), PyObjectIndex(1)])),
                PyObject::SmallInt(1),
            ],
            "Incorrectly unmarshalled self referential tuple (1,1)"
        )
    }

    #[test]
    /// Tests unmarshalling a tuple where one element is a reference to the other
    /// The input bytestring is directly from marshal.dumps((1,1,2))
    fn unmarshal_tuple_with_self_reference2() {
        let res =
            Unmarshaller::loads(b"\xa9\x03\xe9\x01\x00\x00\x00r\x01\x00\x00\x00i\x02\x00\x00\x00");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling short tuple (1, 1, 2) failed, {res:?}");
        };
        assert_eq!(
            objects.as_slice(),
            &[
                PyObject::Tuple(Box::new([
                    PyObjectIndex(1),
                    PyObjectIndex(1),
                    PyObjectIndex(2)
                ])),
                PyObject::SmallInt(1),
                PyObject::SmallInt(2)
            ],
            "Incorrectly unmarshalled self referential tuple (1, 1, 2)"
        )
    }

    #[test]
    fn unmarshal_list() {
        let res = Unmarshaller::loads(b"[\x02\x00\x00\x00\xe9\x01\x00\x00\x00r\x00\x00\x00\x00");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling list [1, 1] failed, {res:?}");
        };

        assert_eq!(
            objects.as_slice(),
            &[
                PyObject::List(Box::new([PyObjectIndex(1), PyObjectIndex(1)])),
                PyObject::SmallInt(1),
            ],
            "Incorrectly unmarshalled list [1, 1]"
        );
    }

    #[test]
    fn unmarshal_list_eof() {
        let res = Unmarshaller::loads(b"[\x10\x00\x00\x00NNN");

        assert_eq!(
            res,
            Err(UnmarshalError::UnexpectedEof),
            "Expected eof while parsing [None, None, None] as a 0x10 item list"
        );
    }

    #[test]
    fn unmarshal_set() {
        let res = Unmarshaller::loads(b"<\x02\x00\x00\x00\xe9\x01\x00\x00\x00\xe9\x02\x00\x00\x00");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling set {{1, 2}} failed, {res:?}");
        };

        assert_eq!(
            objects.as_slice(),
            &[
                PyObject::Set(Box::new([PyObjectIndex(1), PyObjectIndex(2)])),
                PyObject::SmallInt(1),
                PyObject::SmallInt(2),
            ],
            "Incorrectly unmarshalled set {{1, 2}}"
        );
    }

    #[test]
    fn unmarshal_set_eof() {
        let res = Unmarshaller::loads(b"<\x10\x00\x00\x00NTF");

        assert_eq!(
            res,
            Err(UnmarshalError::UnexpectedEof),
            "Expected eof while parsing {{None, True, False}} as a 0x10 item set"
        );
    }

    #[test]
    fn unmarshal_set_duplicates() {
        let res = Unmarshaller::loads(b"<\x02\x00\x00\x00\xe9\x01\x00\x00\x00r\x00\x00\x00\x00");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling set {{1, 1}} failed, {res:?}");
        };

        assert_eq!(
            objects.as_slice(),
            &[
                PyObject::Set(Box::new([PyObjectIndex(1)])),
                PyObject::SmallInt(1),
            ],
            "Incorrectly unmarshalled set {{1, 1}}"
        );
    }

    #[test]
    fn unmarshal_frozen_set() {
        let res = Unmarshaller::loads(b">\x02\x00\x00\x00\xe9\x01\x00\x00\x00\xe9\x02\x00\x00\x00");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling frozen set {{1, 2}} failed, {res:?}");
        };

        assert_eq!(
            objects.as_slice(),
            &[
                PyObject::FrozenSet(Box::new([PyObjectIndex(1), PyObjectIndex(2)])),
                PyObject::SmallInt(1),
                PyObject::SmallInt(2),
            ],
            "Incorrectly unmarshalled frozen set {{1, 2}}"
        );
    }

    #[test]
    fn unmarshal_frozen_set_eof() {
        let res = Unmarshaller::loads(b">\x10\x00\x00\x00NTF");

        assert_eq!(
            res,
            Err(UnmarshalError::UnexpectedEof),
            "Expected eof while parsing {{None, True, False}} as a 0x10 item frozen set"
        );
    }

    #[test]
    fn unmarshal_frozen_set_duplicates() {
        let res = Unmarshaller::loads(b">\x02\x00\x00\x00\xe9\x01\x00\x00\x00r\x00\x00\x00\x00");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling frozen set {{1, 1}} failed, {res:?}");
        };

        assert_eq!(
            objects.as_slice(),
            &[
                PyObject::FrozenSet(Box::new([PyObjectIndex(1)])),
                PyObject::SmallInt(1),
            ],
            "Incorrectly unmarshalled frozen_set {{1, 1}}"
        );
    }

    #[test]
    fn unmarshal_dict() {
        let res = Unmarshaller::loads(b"{\xda\x01a\xe9\x01\x00\x00\x00\xda\x01br\x00\x00\x00\x000");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling {{\"a\":1,\"b\":\"a\"}} failed, {res:?}")
        };

        assert_eq!(
            objects.as_slice(),
            &[
                PyObject::Dict(Box::new([
                    (PyObjectIndex(1), PyObjectIndex(2)),
                    (PyObjectIndex(3), PyObjectIndex(1)),
                ])),
                PyObject::String("a".into()),
                PyObject::SmallInt(1),
                PyObject::String("b".into()),
            ]
        )
    }

    #[test]
    fn unmarshal_dict_eof() {
        let res = Unmarshaller::loads(b"{\xda\x01a\xe9\x01\x00\x00\x00\xda\x01br\x00\x00\x00\x00");
        assert_eq!(res, Err(UnmarshalError::UnexpectedEof));
    }

    #[test]
    /// Test that basic code object demarshalling is implemented correctly
    /// Bytestring is from:
    /// ```python
    /// def f():
    ///     return 5
    /// marshal.dumps(f.__code__)
    /// ```
    fn unmarshal_trivial_code() {
        let res = Unmarshaller::loads(b"\xe3\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x01\x00\x00\x00\x03\x00\x00\x00\xf3\x06\x00\x00\x00\x80\x00^\x05#\x00)\x01\xe9\x05\x00\x00\x00\xa9\x00r\x03\x00\x00\x00\xf3\x00\x00\x00\x00\xda\x07example\xda\x01fr\x06\x00\x00\x00\x01\x00\x00\x00s\x05\x00\x00\x00\x80\x00\xd9\x0b\x0cr\x04\x00\x00\x00");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling function f (equiv to lambda: 5) failed, {res:?}")
        };

        assert_eq!(
            objects.as_slice(),
            &[
                PyObject::Code(CodeObjectConstructor {
                    arg_count: 0,
                    pos_only_arg_count: 0,
                    kw_only_arg_count: 0,
                    stack_size: 1,
                    flags: 0x03,
                    code: PyObjectIndex(1),
                    consts: PyObjectIndex(2),
                    names: PyObjectIndex(4),
                    locals_plus_names: PyObjectIndex(4),
                    locals_plus_kinds: PyObjectIndex(5),
                    filename: PyObjectIndex(6),
                    name: PyObjectIndex(7),
                    qualified_name: PyObjectIndex(7),
                    first_line_no: 1,
                    line_table: PyObjectIndex(8),
                    exception_table: PyObjectIndex(5),
                }),
                PyObject::Bytes(b"\x80\x00^\x05#\x00".as_slice().into()),
                PyObject::Tuple(Box::new([PyObjectIndex(3)])),
                PyObject::SmallInt(5),
                PyObject::Tuple(Box::new([])),
                PyObject::Bytes(Box::new([])),
                PyObject::String("example".into()),
                PyObject::String("f".into()),
                PyObject::Bytes(b"\x80\x00\xd9\x0b\x0c".as_slice().into()),
            ]
        )
    }

    #[test]
    /// Test that the identity function is demarshalled correctly
    /// ```python
    /// def f(x):
    ///     return x
    /// marshal.dumps(f.__code__)
    /// ```
    fn unmarshal_identity_fn_code() {
        let res = Unmarshaller::loads(b"\xe3\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x01\x00\x00\x00\x03\x00\x00\x00\xf3\x06\x00\x00\x00\x80\x00V\x00#\x00)\x01N\xa9\x00)\x01\xda\x01xs\x01\x00\x00\x00&\xda\x07example\xda\x01fr\x05\x00\x00\x00\x01\x00\x00\x00s\x07\x00\x00\x00\x80\x00\xd8\x0b\x0c\x80H\xf3\x00\x00\x00\x00");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling identity function failed, {res:?}")
        };

        assert_eq!(
            objects.as_slice(),
            &[
                PyObject::Code(CodeObjectConstructor {
                    arg_count: 1,
                    pos_only_arg_count: 0,
                    kw_only_arg_count: 0,
                    stack_size: 1,
                    flags: 0x03,
                    code: PyObjectIndex(1),
                    consts: PyObjectIndex(2),
                    names: PyObjectIndex(4),
                    locals_plus_names: PyObjectIndex(5),
                    locals_plus_kinds: PyObjectIndex(7),
                    filename: PyObjectIndex(8),
                    name: PyObjectIndex(9),
                    qualified_name: PyObjectIndex(9),
                    first_line_no: 1,
                    line_table: PyObjectIndex(10),
                    exception_table: PyObjectIndex(11),
                }),
                PyObject::Bytes(b"\x80\x00V\x00#\x00".as_slice().into()),
                PyObject::Tuple(Box::new([PyObjectIndex(3)])),
                PyObject::None,
                PyObject::Tuple(Box::new([])),
                PyObject::Tuple(Box::new([PyObjectIndex(6)])),
                PyObject::String("x".into()),
                PyObject::Bytes(Box::new([0x26])),
                PyObject::String("example".into()),
                PyObject::String("f".into()),
                PyObject::Bytes(b"\x80\x00\xd8\x0b\x0c\x80H".as_slice().into()),
                PyObject::Bytes(Box::new([])),
            ]
        )
    }

    #[test]
    /// Test that closure functions are demarshalled correctly
    /// ```python
    /// def f(x):
    ///     def g(y):
    ///         return x+y
    ///     return g
    /// marshal.dumps(f(1).__code__)
    /// ```
    fn unmarshal_closure_fn_code() {
        let res = Unmarshaller::loads(b"\xe3\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x00\x00\x13\x00\x00\x00\xf3\x16\x00\x00\x00<\x01\x80\x00S\x01V\x00,\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00#\x00)\x01N\xa9\x00)\x02\xda\x01y\xda\x01xs\x02\x00\x00\x00&\x80\xda\x07example\xda\x01g\xda\x0cf.<locals>.g\x02\x00\x00\x00s\x0c\x00\x00\x00\xf8\x80\x00\xd8\x0f\x10\x90\x11\x8ds\x88\n\xf3\x00\x00\x00\x00");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling identity function failed, {res:?}")
        };

        assert_eq!(
            objects.as_slice(),
            &[
                PyObject::Code(CodeObjectConstructor {
                    arg_count: 1,
                    pos_only_arg_count: 0,
                    kw_only_arg_count: 0,
                    stack_size: 2,
                    flags: 0x13,
                    code: PyObjectIndex(1),
                    consts: PyObjectIndex(2),
                    names: PyObjectIndex(4),
                    locals_plus_names: PyObjectIndex(5),
                    locals_plus_kinds: PyObjectIndex(8),
                    filename: PyObjectIndex(9),
                    name: PyObjectIndex(10),
                    qualified_name: PyObjectIndex(11),
                    first_line_no: 2,
                    line_table: PyObjectIndex(12),
                    exception_table: PyObjectIndex(13),
                }),
                PyObject::Bytes(
                    b"<\x01\x80\x00S\x01V\x00,\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00#\x00"
                        .as_slice()
                        .into()
                ),
                PyObject::Tuple(Box::new([PyObjectIndex(3)])),
                PyObject::None,
                PyObject::Tuple(Box::new([])),
                PyObject::Tuple(Box::new([PyObjectIndex(6), PyObjectIndex(7)])),
                PyObject::String("y".into()),
                PyObject::String("x".into()),
                PyObject::Bytes(Box::new([0x26, 0x80])),
                PyObject::String("example".into()),
                PyObject::String("g".into()),
                PyObject::String("f.<locals>.g".into()),
                PyObject::Bytes(
                    b"\xf8\x80\x00\xd8\x0f\x10\x90\x11\x8ds\x88\n"
                        .as_slice()
                        .into()
                ),
                PyObject::Bytes(Box::new([])),
            ]
        )
    }

    #[test]
    /// Test that closure functions are demarshalled correctly
    /// ```python
    /// def f(x):
    ///     def g(y):
    ///         return x+y
    ///     return g
    /// marshal.dumps(f.__code__)
    /// ```
    fn unmarshal_nested_fn() {
        let res = Unmarshaller::loads(b"\xe3\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x00\x00\x03\x00\x00\x00\xf3\x14\x00\x00\x00a\x00\x80\x00V\x003\x01R\x00\x17\x00l\x08p\x01V\x01#\x00)\x01\xe3\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x00\x00\x13\x00\x00\x00\xf3\x16\x00\x00\x00<\x01\x80\x00S\x01V\x00,\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00#\x00)\x01N\xa9\x00)\x02\xda\x01y\xda\x01xs\x02\x00\x00\x00&\x80\xda\x07example\xda\x01g\xda\x0cf.<locals>.g\x02\x00\x00\x00s\x0c\x00\x00\x00\xf8\x80\x00\xd8\x0f\x10\x90\x11\x8ds\x88\n\xf3\x00\x00\x00\x00r\x04\x00\x00\x00)\x02r\x06\x00\x00\x00r\x08\x00\x00\x00s\x02\x00\x00\x00f r\x07\x00\x00\x00\xda\x01fr\x0b\x00\x00\x00\x01\x00\x00\x00s\x0d\x00\x00\x00\xf8\x80\x00\xf5\x02\x01\x05\x13\xe0\x0b\x0c\x80Hr\x0a\x00\x00\x00");
        let Ok(PyObjectRegion(objects)) = res else {
            panic!("Unmarshalling identity function failed, {res:?}")
        };

        assert_eq!(
            objects.as_slice(),
            &[
                PyObject::Code(CodeObjectConstructor {
                    arg_count: 1,
                    pos_only_arg_count: 0,
                    kw_only_arg_count: 0,
                    stack_size: 2,
                    flags: 3,
                    code: PyObjectIndex(1),
                    consts: PyObjectIndex(2),
                    names: PyObjectIndex(7),
                    locals_plus_names: PyObjectIndex(17),
                    locals_plus_kinds: PyObjectIndex(18),
                    filename: PyObjectIndex(12),
                    name: PyObjectIndex(19),
                    qualified_name: PyObjectIndex(19),
                    first_line_no: 1,
                    line_table: PyObjectIndex(20),
                    exception_table: PyObjectIndex(16),
                }),
                PyObject::Bytes(
                    b"a\x00\x80\x00V\x003\x01R\x00\x17\x00l\x08p\x01V\x01#\x00"
                        .as_slice()
                        .into()
                ),
                PyObject::Tuple(Box::new([PyObjectIndex(3)])),
                PyObject::Code(CodeObjectConstructor {
                    arg_count: 1,
                    pos_only_arg_count: 0,
                    kw_only_arg_count: 0,
                    stack_size: 2,
                    flags: 0x13,
                    code: PyObjectIndex(4),
                    consts: PyObjectIndex(5),
                    names: PyObjectIndex(7),
                    locals_plus_names: PyObjectIndex(8),
                    locals_plus_kinds: PyObjectIndex(11),
                    filename: PyObjectIndex(12),
                    name: PyObjectIndex(13),
                    qualified_name: PyObjectIndex(14),
                    first_line_no: 2,
                    line_table: PyObjectIndex(15),
                    exception_table: PyObjectIndex(16),
                }),
                PyObject::Bytes(
                    b"<\x01\x80\x00S\x01V\x00,\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00#\x00"
                        .as_slice()
                        .into()
                ),
                PyObject::Tuple(Box::new([PyObjectIndex(6)])),
                PyObject::None,
                PyObject::Tuple(Box::new([])),
                PyObject::Tuple(Box::new([PyObjectIndex(9), PyObjectIndex(10)])),
                PyObject::String("y".into()),
                PyObject::String("x".into()),
                PyObject::Bytes(Box::new([0x26, 0x80])),
                PyObject::String("example".into()),
                PyObject::String("g".into()),
                PyObject::String("f.<locals>.g".into()),
                PyObject::Bytes(
                    b"\xf8\x80\x00\xd8\x0f\x10\x90\x11\x8ds\x88\n"
                        .as_slice()
                        .into()
                ),
                PyObject::Bytes(Box::new([])),
                PyObject::Tuple(Box::new([PyObjectIndex(10), PyObjectIndex(13)])),
                PyObject::Bytes(Box::new([0x66, 0x20])),
                PyObject::String("f".into()),
                PyObject::Bytes(
                    b"\xf8\x80\x00\xf5\x02\x01\x05\x13\xe0\x0b\x0c\x80H"
                        .as_slice()
                        .into()
                ),
            ]
        )
    }

    #[test]
    fn unmarshal_explicit_unknown() {
        let res = Unmarshaller::loads(b"?");
        assert_eq!(res, Err(UnmarshalError::ExplicitUnknown));
    }

    #[test]
    fn unmarshal_asciis() {
        let resa = Unmarshaller::loads(b"a\x03\x00\x00\x00abc");
        let resai = Unmarshaller::loads(b"a\x03\x00\x00\x00abc");
        let resas = Unmarshaller::loads(b"a\x03\x00\x00\x00abc");
        let resasi = Unmarshaller::loads(b"a\x03\x00\x00\x00abc");

        assert_eq!(
            resa, resai,
            "Interned and non-interned ascii string 'abc' should match"
        );
        assert_eq!(
            resa, resas,
            "Short and normal ascii string 'abc' should match"
        );
        assert_eq!(
            resa, resasi,
            "Interned and non-interned ascii string 'abc' should match"
        );

        let Ok(PyObjectRegion(objects)) = resa else {
            panic!("Unmarshalling ascii \"abc\" function failed, {resa:?}")
        };

        assert_eq!(objects, &[PyObject::String("abc".into())])
    }
}
