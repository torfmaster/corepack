use std::ops::{Deref, DerefMut};
use std::iter::Iterator;

use alloc::boxed::Box;

use collections::{String, Vec};

use serde::{Serialize, Deserialize, Serializer, Deserializer, Error};

use serde::{ser, de};

use error;

#[derive(Debug, Clone)]
pub enum Generic {
    Nil,
    False,
    True,
    Int(i64),
    UInt(u64),
    F32(f32),
    F64(f64),
    Bin(Box<[u8]>),
    Str(Box<str>),
    Array(Box<[Generic]>),
    Map(Box<[(Generic, Generic)]>),
}

struct SeqVisitor<I: Iterator<Item=Generic>> {
    iter: I
}

struct MapVisitor<I: Iterator<Item=(Generic, Generic)>> {
    iter: I,
    value: Option<Generic>
}

struct VariantVisitor<'a> {
    parent: &'a mut Generic
}

struct MapGeneric {
    keys: VecGeneric,
    values: VecGeneric,
}

struct VecGeneric(Vec<Generic>);

pub struct GenericVisitor;

impl<'a> de::VariantVisitor for VariantVisitor<'a> {
    type Error = error::Error;

    fn visit_variant<V>(&mut self) -> Result<V, error::Error> where V: Deserialize {
        // unit variants are just a string, and we don't need to deconstruct them
        if self.parent.is_str() {
            return V::deserialize(self.parent) .map_err(|e| error::Error::chain(
                error::Reason::Other,
                format!("Failed to deserialize variant"),
                Some(Box::new(e))
            ));
        }

        match self.parent {
            // variants of other types are single-entry maps
            &mut Generic::Map(ref mut m) => {
                if m.len() != 1 {
                    // invariant violated
                    return Err(error::Error::invalid_length(m.len()));
                }

                V::deserialize(&mut m[0].0).map_err(|e| error::Error::chain(
                    error::Reason::Other,
                    format!("Failed to deserialize variant"),
                    Some(Box::new(e))
                ))
            },
            // other types are invalid
            _ => Err(error::Error::invalid_type(de::Type::Enum))
        }
    }

    fn visit_newtype<T>(&mut self) -> Result<T, error::Error> where T: Deserialize {
        match self.parent {
            &mut Generic::Map(ref mut m) => {
                if m.len() != 1 {
                    // not enough items
                    return Err(error::Error::invalid_length(m.len()))
                }

                T::deserialize(&mut m[0].1).map_err(|e| error::Error::chain(
                    error::Reason::Other,
                    format!("Failed to deserialize newtype"),
                    Some(Box::new(e))
                ))
            },
            _ => Err(error::Error::invalid_type(de::Type::Enum))
        }
    }

    fn visit_tuple<V>(&mut self, _: usize, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        match self.parent {
            &mut Generic::Map(ref mut m) => {
                if m.len() != 1 {
                    // not enough items
                    return Err(error::Error::invalid_length(m.len()))
                }

                m[0].1.deserialize(visitor)
            },
            _ => Err(error::Error::invalid_type(de::Type::Enum))
        }
    }

    fn visit_struct<V>(&mut self, fields: &'static [&'static str], visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        // This is _maybe_ the right thing to do
        self.visit_tuple(fields.len(), visitor)
    }

    fn visit_unit(&mut self) -> Result<(), error::Error> {
        Ok(())
    }
}

impl<I: Iterator<Item=Generic>> de::SeqVisitor for SeqVisitor<I> {
    type Error = error::Error;

    fn visit<T>(&mut self) -> Result<Option<T>, error::Error> where T: Deserialize {
        if let Some(mut item) = self.iter.next() {
            Ok(Some(try!(T::deserialize(&mut item))))
        } else {
            Ok(None)
        }
    }

    fn end(&mut self) -> Result<(), error::Error> {
        if self.iter.next().is_none() {
            Ok(())
        } else {
            Err(de::Error::invalid_length(self.size_hint().0))
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<I: Iterator<Item=(Generic, Generic)>> de::MapVisitor for MapVisitor<I> {
    type Error = error::Error;

    fn visit_key<K>(&mut self) -> Result<Option<K>, error::Error> where K: Deserialize {
        let item;

        if let Some(next) = self.iter.next() {
            item = next;
        } else {
            return Ok(None);
        }

        let (mut key, value) = item;

        self.value = Some(value);
        Ok(Some(try!(K::deserialize(&mut key))))
    }

    fn visit_value<V>(&mut self) -> Result<V, error::Error> where V: Deserialize {
        if let Some(mut value) = self.value.take() {
            Ok(try!(V::deserialize(&mut value)))
        } else {
            Err(de::Error::end_of_stream())
        }
    }

    fn visit<K, V>(&mut self) -> Result<Option<(K, V)>, error::Error> where K: Deserialize, V: Deserialize {
        if let Some((mut key, mut value)) = self.iter.next() {
            Ok(Some((try!(K::deserialize(&mut key)), try!(V::deserialize(&mut value)))))
        } else {
            Ok(None)
        }
    }

    fn end(&mut self) -> Result<(), error::Error> {
        if self.iter.next().is_none() {
            Ok(())
        } else {
            Err(de::Error::invalid_length(self.size_hint().0))
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl Deref for VecGeneric {
    type Target = Vec<Generic>;

    fn deref(&self) -> &Vec<(Generic)> {
        &self.0
    }
}

impl DerefMut for VecGeneric {
    fn deref_mut(&mut self) -> &mut Vec<Generic> {
        &mut self.0
    }
}

impl de::Visitor for GenericVisitor {
    type Value = Generic;

    fn visit_bool<E>(&mut self, v: bool) -> Result<Generic, E> where E: Error {
        if v {
            Ok(Generic::True)
        } else {
            Ok(Generic::False)
        }
    }

    fn visit_i64<E>(&mut self, v: i64) -> Result<Generic, E> where E: Error {
        Ok(Generic::Int(v))
    }

    fn visit_u64<E>(&mut self, v: u64) -> Result<Generic, E> where E: Error {
        Ok(Generic::UInt(v))
    }

    fn visit_f32<E>(&mut self, v: f32) -> Result<Generic, E> where E: Error {
        Ok(Generic::F32(v))
    }

    fn visit_f64<E>(&mut self, v: f64) -> Result<Generic, E> where E: Error {
        Ok(Generic::F64(v))
    }

    fn visit_str<E>(&mut self, v: &str) -> Result<Generic, E> where E: Error {
        Ok(Generic::Str(String::from(v).into_boxed_str()))
    }

    fn visit_string<E>(&mut self, v: String) -> Result<Generic, E> where E: Error {
        Ok(Generic::Str(v.into_boxed_str()))
    }

    fn visit_unit<E>(&mut self) -> Result<Generic, E> where E: Error {
        Ok(Generic::Nil)
    }

    fn visit_none<E>(&mut self) -> Result<Generic, E> where E: Error {
        self.visit_unit()
    }

    fn visit_some<D>(&mut self, d: &mut D) -> Result<Generic, D::Error> where D: Deserializer {
        d.deserialize(GenericVisitor)
    }

    fn visit_newtype_struct<D>(&mut self, d: &mut D) -> Result<Generic, D::Error> where D: Deserializer {
        d.deserialize(GenericVisitor)
    }

    fn visit_map<V>(&mut self, mut v: V) -> Result<Generic, V::Error> where V: de::MapVisitor {
        let mut buf = vec![];

        while let Some(pair) = try!(v.visit::<Generic, Generic>()) {
            buf.push(pair);
        }

        Ok(Generic::Map(buf.into_boxed_slice()))
    }

    fn visit_seq<V>(&mut self, mut v: V) -> Result<Generic, V::Error> where V: de::SeqVisitor {
        let mut buf = vec![];

        while let Some(item) = try!(v.visit::<Generic>()) {
            buf.push(item);
        }

        Ok(Generic::Array(buf.into_boxed_slice()))
    }

    fn visit_bytes<E>(&mut self, v: &[u8]) -> Result<Generic, E> where E: Error {
        Ok(Generic::Bin(Vec::from(v).into_boxed_slice()))
    }

    fn visit_byte_buf<E>(&mut self, v: Vec<u8>) -> Result<Generic, E> where E: Error {
        Ok(Generic::Bin(v.into_boxed_slice()))
    }
}

impl Serialize for Generic {
    fn serialize<S>(&self, s: &mut S) -> Result<(), S::Error> where S: Serializer {
        use self::Generic::*;

        match self {
            &Nil => s.serialize_unit(),
            &False => s.serialize_bool(false),
            &True => s.serialize_bool(true),
            &Int(i) => s.serialize_i64(i),
            &UInt(i) => s.serialize_u64(i),
            &F32(f) => s.serialize_f32(f),
            &F64(f) => s.serialize_f64(f),
            &Bin(ref b) => s.serialize_bytes(b),
            &Str(ref st) => s.serialize_str(st),
            &Array(ref a) => {
                let mut state = try!(s.serialize_seq(Some(a.len())));
                for item in a.iter().cloned() {
                    try!(s.serialize_seq_elt(&mut state, item));
                }
                s.serialize_seq_end(state)
            },
            &Map(ref m) => {
                let mut state = try!(s.serialize_map(Some(m.len())));
                for (key, value) in m.iter().cloned() {
                    try!(s.serialize_map_key(&mut state, key));
                    try!(s.serialize_map_value(&mut state, value));
                }
                s.serialize_map_end(state)
            }
        }
    }
}

impl Deserialize for Generic {
    fn deserialize<D>(d: &mut D) -> Result<Generic, D::Error> where D: Deserializer {
        d.deserialize(GenericVisitor)
    }
}

impl de::Deserializer for Generic {
    type Error = error::Error;

    fn deserialize<V>(&mut self, mut v: V) -> Result<V::Value, error::Error> where V: de::Visitor {
        use self::Generic::*;

        match self {
            &mut Nil => v.visit_unit(),
            &mut False => v.visit_bool(false),
            &mut True => v.visit_bool(true),
            &mut Int(i) => v.visit_i64(i),
            &mut UInt(i) => v.visit_u64(i),
            &mut F32(f) => v.visit_f32(f),
            &mut F64(f) => v.visit_f64(f),
            &mut Bin(ref b) => v.visit_bytes(&b),
            &mut Str(ref s) => v.visit_str(&s),
            &mut Array(ref a) => v.visit_seq(SeqVisitor {
                iter: a.iter().cloned()
            }),
            &mut Map(ref m) => v.visit_map(MapVisitor {
                iter: m.iter().cloned(),
                value: None
            })
        }
    }

    
    fn deserialize_bool<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize(visitor)
    }

    fn deserialize_u64<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize(visitor)
    }

    fn deserialize_usize<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u8<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize_u64(visitor)
    }

    fn deserialize_i64<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize(visitor)
    }

    fn deserialize_isize<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i8<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize_i64(visitor)
    }

    fn deserialize_f64<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize(visitor)
    }

    fn deserialize_f32<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize_f64(visitor)
    }

    fn deserialize_str<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize(visitor)
    }

    fn deserialize_char<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize_str(visitor)
    }

    fn deserialize_string<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize_str(visitor)
    }

    fn deserialize_unit<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize(visitor)
    }

    fn deserialize_option<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize(visitor)
    }

    fn deserialize_seq<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize(visitor)
    }

    fn deserialize_seq_fixed_size<V>(&mut self, _: usize, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize_seq(visitor)
    }

    fn deserialize_bytes<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize(visitor)
    }

    fn deserialize_map<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize(visitor)
    }

    fn deserialize_unit_struct<V>(&mut self, _: &'static str, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(&mut self, _: &'static str, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize(visitor)
    }

    fn deserialize_tuple_struct<V>(&mut self, _: &'static str, len: usize, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize_tuple(len, visitor)
    }

    fn deserialize_struct<V>(&mut self, _: &'static str, _: &'static [&'static str], visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize_map(visitor)
    }

    fn deserialize_struct_field<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize(visitor)
    }

    fn deserialize_tuple<V>(&mut self, len: usize, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize_seq_fixed_size(len, visitor)
    }

    fn deserialize_enum<V>(&mut self, _: &'static str, _: &'static [&'static str], mut visitor: V) -> Result<V::Value, error::Error>
        where V: de::EnumVisitor {
        visitor.visit(VariantVisitor {
            parent: self
        })
    }

    fn deserialize_ignored_any<V>(&mut self, visitor: V) -> Result<V::Value, error::Error>
        where V: de::Visitor {
        self.deserialize(visitor)
    }
}

impl ser::Serializer for VecGeneric {
    type Error = error::Error;

    type SeqState = VecGeneric;
    type TupleState = VecGeneric;
    type TupleStructState = VecGeneric;
    type TupleVariantState = (&'static str, VecGeneric);

    type MapState = MapGeneric;
    type StructState = MapGeneric;
    type StructVariantState = (&'static str, MapGeneric);

    fn serialize_bool(&mut self, v: bool) -> Result<(), error::Error> {
        if v {
            self.push(Generic::True);
        } else {
            self.push(Generic::False);
        }

        Ok(())
    }

    fn serialize_i64(&mut self, v: i64) -> Result<(), error::Error> {
        self.push(Generic::Int(v));

        Ok(())
    }

    fn serialize_isize(&mut self, value: isize) -> Result<(), error::Error> {
        self.serialize_i64(value as i64)
    }

    fn serialize_i8(&mut self, value: i8) -> Result<(), error::Error> {
        self.serialize_i64(value as i64)
    }

    fn serialize_i16(&mut self, value: i16) -> Result<(), error::Error> {
        self.serialize_i64(value as i64)
    }

    fn serialize_i32(&mut self, value: i32) -> Result<(), error::Error> {
        self.serialize_i64(value as i64)
    }

    fn serialize_u64(&mut self, v: u64) -> Result<(), error::Error> {
        self.push(Generic::UInt(v));

        Ok(())
    }

    fn serialize_usize(&mut self, value: usize) -> Result<(), error::Error> {
        self.serialize_u64(value as u64)
    }

    fn serialize_u8(&mut self, value: u8) -> Result<(), error::Error> {
        self.serialize_u64(value as u64)
    }

    fn serialize_u16(&mut self, value: u16) -> Result<(), error::Error> {
        self.serialize_u64(value as u64)
    }

    fn serialize_u32(&mut self, value: u32) -> Result<(), error::Error> {
        self.serialize_u64(value as u64)
    }

    fn serialize_f32(&mut self, f: f32) -> Result<(), error::Error> {
        self.push(Generic::F32(f));

        Ok(())
    }

    fn serialize_f64(&mut self, f: f64) -> Result<(), error::Error> {
        self.push(Generic::F64(f));

        Ok(())
    }

    fn serialize_str(&mut self, value: &str) -> Result<(), error::Error> {
        self.push(Generic::Str(String::from(value).into_boxed_str()));

        Ok(())
    }

    fn serialize_char(&mut self, value: char) -> Result<(), error::Error> {
        let string = String::from(vec![value]);
        self.serialize_str(&*string)
    }

    fn serialize_bytes(&mut self, value: &[u8]) -> Result<(), error::Error> {
        self.push(Generic::Bin(Vec::from(value).into_boxed_slice()));

        Ok(())
    }

    fn serialize_unit(&mut self) -> Result<(), error::Error> {
        self.push(Generic::Nil);

        Ok(())
    }

    fn serialize_unit_struct(&mut self, _: &'static str) -> Result<(), error::Error> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(&mut self, _: &'static str, _: usize, variant: &'static str) -> Result<(), error::Error> {
        self.serialize_str(variant)
    }

    fn serialize_newtype_struct<T>(&mut self, name: &'static str, value: T) -> Result<(), error::Error>
        where T: Serialize {
        let mut state = try!(self.serialize_tuple_struct(name, 1));
        try!(self.serialize_tuple_struct_elt(&mut state, value));
        self.serialize_tuple_struct_end(state)
    }

    fn serialize_newtype_variant<T>(&mut self, name: &'static str, variant_index: usize, variant: &'static str, value: T) -> Result<(), error::Error>
        where T: Serialize {
        let mut state = try!(self.serialize_tuple_variant(name, variant_index, variant, 1));
        try!(self.serialize_tuple_variant_elt(&mut state, value));

        // serialize the newtype directly, rather than putting it in an array
        if (state.1).0.len() != 1 {
            // we got an incorrect number of items
            return Err(error::Error::new(
                error::Reason::BadLength,
                format!("Newtype variant serialized into {} items instead of exactly one",
                        (state.1).0.len()))
            );
        }

        self.push(Generic::Map(vec![(
            Generic::Str(String::from(state.0).into_boxed_str()),
            (state.1).0.pop().unwrap(),
        )].into_boxed_slice()));

        Ok(())
    }

    fn serialize_none(&mut self) -> Result<(), error::Error> {
        self.serialize_unit()
    }

    fn serialize_some<V>(&mut self, value: V) -> Result<(), error::Error> where V: Serialize {
        value.serialize(self)
    }

    fn serialize_seq(&mut self, len: Option<usize>) -> Result<VecGeneric, error::Error> {
        if let Some(capacity) = len {
            Ok(VecGeneric(Vec::with_capacity(capacity)))
        } else {
            Ok(VecGeneric(vec![]))
        }
    }

    fn serialize_seq_fixed_size(&mut self, size: usize) -> Result<VecGeneric, error::Error> {
        self.serialize_seq(Some(size))
    }

    fn serialize_seq_elt<T>(&mut self, state: &mut VecGeneric, value: T) -> Result<(), error::Error> where T: Serialize {
        value.serialize(state)
    }

    fn serialize_seq_end(&mut self, state: VecGeneric) -> Result<(), error::Error> {
        self.push(Generic::Array(state.0.into_boxed_slice()));

        Ok(())
    }

    fn serialize_tuple(&mut self, len: usize) -> Result<VecGeneric, error::Error> {
        self.serialize_seq_fixed_size(len)
    }

    fn serialize_tuple_elt<T>(&mut self, state: &mut VecGeneric, value: T) -> Result<(), error::Error>
        where T: Serialize {
        self.serialize_seq_elt(state, value)
    }

    fn serialize_tuple_end(&mut self, state: VecGeneric) -> Result<(), error::Error> {
        self.serialize_seq_end(state)
    }

    fn serialize_tuple_struct(&mut self, _: &'static str, len: usize) -> Result<VecGeneric, error::Error> {
        self.serialize_tuple(len)
    }

    fn serialize_tuple_struct_elt<T>(&mut self, state: &mut VecGeneric, value: T) -> Result<(), error::Error>
        where T: Serialize {
        self.serialize_tuple_elt(state, value)
    }

    fn serialize_tuple_struct_end(&mut self, state: VecGeneric) -> Result<(), error::Error> {
        self.serialize_tuple_end(state)
    }

    fn serialize_tuple_variant(&mut self, name: &'static str, _: usize, variant: &'static str, len: usize) -> Result<Self::TupleVariantState, error::Error> {
        Ok((variant, try!(self.serialize_tuple_struct(name, len))))
    }

    fn serialize_tuple_variant_elt<T>(&mut self, state: &mut Self::TupleVariantState, value: T) -> Result<(), error::Error>
        where T: Serialize {
        self.serialize_tuple_struct_elt(&mut state.1, value)
    }

    fn serialize_tuple_variant_end(&mut self, state: Self::TupleVariantState) -> Result<(), error::Error> {
        self.push(Generic::Map(vec![(
            Generic::Str(String::from(state.0).into_boxed_str()),
            Generic::Array((state.1).0.into_boxed_slice()),
        )].into_boxed_slice()));

        Ok(())
    }

    fn serialize_map(&mut self, len: Option<usize>) -> Result<MapGeneric, error::Error> {
        if let Some(capacity) = len {
            Ok(MapGeneric {
                keys: VecGeneric(Vec::with_capacity(capacity)),
                values: VecGeneric(Vec::with_capacity(capacity)),
            })
        } else {
            Ok(MapGeneric {
                keys: VecGeneric(vec![]),
                values: VecGeneric(vec![]),
            })
        }
    }

    fn serialize_map_key<T>(&mut self, state: &mut MapGeneric, key: T) -> Result<(), error::Error> where T: Serialize {
        key.serialize(&mut state.keys)
    }

    fn serialize_map_value<T>(&mut self, state: &mut MapGeneric, value: T) -> Result<(), error::Error> where T: Serialize {
        value.serialize(&mut state.values)
    }

    fn serialize_map_end(&mut self, state: MapGeneric) -> Result<(), error::Error> {
        if state.keys.len() != state.values.len() {
            return Err(error::Error::custom("Number of keys and number of values did not match"));
        }

        self.push(Generic::Map(state.keys.0.into_iter().zip(state.values.0.into_iter())
                               .collect::<Vec<(Generic, Generic)>>().into_boxed_slice()));

        Ok(())
    }

    fn serialize_struct(&mut self, _: &'static str, len: usize) -> Result<MapGeneric, error::Error> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_elt<V>(&mut self, state: &mut MapGeneric, key: &'static str, value: V) -> Result<(), error::Error>
        where V: Serialize {
        try!(self.serialize_map_key(state, key));
        self.serialize_map_value(state, value)
    }

    fn serialize_struct_end(&mut self, state: MapGeneric) -> Result<(), error::Error> {
        self.serialize_map_end(state)
    }

    fn serialize_struct_variant(&mut self, _: &'static str, _: usize, variant: &'static str, len: usize) -> Result<Self::StructVariantState, error::Error> {
        Ok((variant, MapGeneric {
            keys: VecGeneric(Vec::with_capacity(len)),
            values: VecGeneric(Vec::with_capacity(len))
        }))
    }

    fn serialize_struct_variant_elt<V>(&mut self, state: &mut Self::StructVariantState, key: &'static str, value: V) -> Result<(), error::Error>
        where V: Serialize {
        try!(self.serialize_map_key(&mut state.1, key));
        self.serialize_map_value(&mut state.1, value)
    }

    fn serialize_struct_variant_end(&mut self, state: Self::StructVariantState) -> Result<(), error::Error> {
        let (variant, map) = state;

        if map.keys.len() != map.values.len() {
            return Err(error::Error::custom("Number of keys and number of values did not match"));
        }

        self.push(Generic::Map(vec![(
            Generic::Str(String::from(variant).into_boxed_str()),
            Generic::Map(map.keys.0.into_iter().zip(map.values.0.into_iter())
                         .collect::<Vec<(Generic, Generic)>>().into_boxed_slice())
        )].into_boxed_slice()));

        Ok(())
    }
}

impl Generic {
    pub fn from_value<V>(value: V) -> Result<Generic, error::Error> where V: Serialize {
        let mut buf = VecGeneric(vec![]);

        try!(value.serialize(&mut buf));

        if let Some(generic) = buf.pop() {
            if !buf.is_empty() {
                Err(error::Error::new(error::Reason::BadLength, "Value serialized into more than one item".into()))
            } else {
                Ok(generic)
            }
        } else {
            Err(error::Error::new(error::Reason::BadLength, "Value serialized into no items".into()))
        }
    }

    pub fn is_nil(&self) -> bool {
        if let &Generic::Nil = self {
            true
        } else {
            false
        }
    }

    pub fn is_false(&self) -> bool {
        if let &Generic::False = self {
            true
        } else {
            false
        }
    }

    pub fn is_true(&self) -> bool {
        if let &Generic::True = self {
            true
        } else {
            false
        }
    }

    pub fn is_int(&self) -> bool {
        if let &Generic::Int(_) = self {
            true
        } else {
            false
        }
    }

    pub fn is_uint(&self) -> bool {
        if let &Generic::UInt(_) = self {
            true
        } else {
            false
        }
    }

    pub fn is_f32(&self) -> bool {
        if let &Generic::F32(_) = self {
            true
        } else {
            false
        }
    }

    pub fn is_f64(&self) -> bool {
        if let &Generic::F64(_) = self {
            true
        } else {
            false
        }
    }

    pub fn is_bin(&self) -> bool {
        if let &Generic::Bin(_) = self {
            true
        } else {
            false
        }
    }

    pub fn is_str(&self) -> bool {
        if let &Generic::Str(_) = self {
            true
        } else {
            false
        }
    }

    pub fn is_array(&self) -> bool {
        if let &Generic::Array(_) = self {
            true
        } else {
            false
        }
    }

    pub fn is_map(&self) -> bool {
        if let &Generic::Map(_) = self {
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test {
    use serde::Deserialize;

    use ::test::T;
    // #[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
    // enum T {
    //     A(usize),
    //     B,
    //     C(i8, i8),
    //     D { a: isize },
    // }

    #[test]
    fn test_enum() {
        let expected = T::B;

        let mut x = ::Generic::from_value(&expected).expect("Failed to serialize enum");

        let actual = T::deserialize(&mut x).expect("Failed to deserialize enum");

        assert_eq!(expected, actual);
    }

    #[test]
    fn test_enum_newtype() {
        let expected = T::A(42);

        let mut x = ::Generic::from_value(&expected).expect("Failed to serialize enum");

        let actual = T::deserialize(&mut x).expect("Failed to deserialize enum");

        assert_eq!(expected, actual);
    }

    #[test]
    fn test_enum_tuple() {
        let expected = T::C(-3, 22);

        let mut x = ::Generic::from_value(&expected).expect("Failed to serialize enum");

        let actual = T::deserialize(&mut x).expect("Failed to deserialize enum");

        assert_eq!(expected, actual);
    }

    #[test]
    fn test_enum_struct() {
        let expected = T::D { a: 9001 };

        let mut x = ::Generic::from_value(&expected).expect("Failed to serialize enum");

        let actual = T::deserialize(&mut x).expect("Failed to deserialize enum");

        assert_eq!(expected, actual);
    }
}
