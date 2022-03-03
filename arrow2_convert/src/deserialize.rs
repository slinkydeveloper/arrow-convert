// Implementations of derive traits for arrow2 built-in types

use arrow2::array::*;
use chrono::{NaiveDate, NaiveDateTime};

use crate::field::*;

/// Implemented by [`ArrowField`] that can be deserialized from arrow
pub trait ArrowDeserialize: ArrowField + Sized
where
    Self::ArrayType: ArrowArray,
    for<'a> &'a Self::ArrayType: IntoIterator,
{
    type ArrayType;

    /// Deserialize this field from arrow
    fn arrow_deserialize(v: <&Self::ArrayType as IntoIterator>::Item) -> Option<<Self as ArrowField>::Type>;

    #[inline]
    // For internal use only
    //
    // This is an ugly hack to allow generating a blanket Option<T> deserialize.
    // Ideally we would be able to capture the optional field of the iterator via
    // something like for<'a> &'a T::ArrayType: IntoIterator<Item=Option<E>>,
    // However, the E parameter seems to confuse the borrow checker if it's a reference.
    fn arrow_deserialize_internal(v: <&Self::ArrayType as IntoIterator>::Item) -> <Self as ArrowField>::Type {
        Self::arrow_deserialize(v).unwrap()
    }
}

/// Internal trait used to support deserialization and iteration of structs, and nested struct lists
///
/// Trivial pass-thru implementations are provided for arrow2 arrays that implement IntoIterator.
///
/// The derive macro generates implementations for typed struct arrays.
#[doc(hidden)]
pub trait ArrowArray
where
    for<'a> &'a Self: IntoIterator,
{
    type BaseArrayType: Array;

    // Returns a typed iterator to the underlying elements of the array from an untyped Array reference.
    fn iter_from_array_ref(b: &dyn Array) -> <&Self as IntoIterator>::IntoIter;
}

// Macro to facilitate implementation for numeric types and numeric arrays.
macro_rules! impl_arrow_deserialize_primitive {
    ($physical_type:ty, $logical_type:ident) => {
        impl ArrowDeserialize for $physical_type {
            type ArrayType = PrimitiveArray<$physical_type>;

            #[inline]
            fn arrow_deserialize<'a>(v: Option<&$physical_type>) -> Option<Self> {
                v.map(|t| *t)
            }
        }

        impl_arrow_array!(PrimitiveArray<$physical_type>);
    };
}

macro_rules! impl_arrow_array {
    ($array:ty) => {
        impl ArrowArray for $array {
            type BaseArrayType = Self;

            fn iter_from_array_ref(b: &dyn Array) -> <&Self as IntoIterator>::IntoIter {
                b.as_any()
                    .downcast_ref::<Self::BaseArrayType>()
                    .unwrap()
                    .into_iter()
            }
        }
    };
}

// blanket implementation for optional fields
impl<T> ArrowDeserialize for Option<T>
where
    T: ArrowDeserialize,
    T::ArrayType: 'static + ArrowArray,
    for<'a> &'a T::ArrayType: IntoIterator,
{
    type ArrayType = <T as ArrowDeserialize>::ArrayType;

    #[inline]
    fn arrow_deserialize(v: <&Self::ArrayType as IntoIterator>::Item) -> Option<<Self as ArrowField>::Type> {
        Some(Self::arrow_deserialize_internal(v))
    }

    #[inline]
    fn arrow_deserialize_internal(v: <&Self::ArrayType as IntoIterator>::Item) -> <Self as ArrowField>::Type {
        <T as ArrowDeserialize>::arrow_deserialize(v)
    }
}

impl_arrow_deserialize_primitive!(u8, UInt8);
impl_arrow_deserialize_primitive!(u16, UInt16);
impl_arrow_deserialize_primitive!(u32, UInt32);
impl_arrow_deserialize_primitive!(u64, UInt64);
impl_arrow_deserialize_primitive!(i8, Int8);
impl_arrow_deserialize_primitive!(i16, Int16);
impl_arrow_deserialize_primitive!(i32, Int32);
impl_arrow_deserialize_primitive!(i64, Int64);
impl_arrow_deserialize_primitive!(f32, Float32);
impl_arrow_deserialize_primitive!(f64, Float64);

impl ArrowDeserialize for String {
    type ArrayType = Utf8Array<i32>;

    #[inline]
    fn arrow_deserialize(v: Option<&str>) -> Option<Self> {
        v.map(|t| t.to_string())
    }
}

impl ArrowDeserialize for LargeString {
    type ArrayType = Utf8Array<i64>;

    #[inline]
    fn arrow_deserialize(v: Option<&str>) -> Option<String> {
        v.map(|t| t.to_string())
    }
}

impl ArrowDeserialize for bool {
    type ArrayType = BooleanArray;

    #[inline]
    fn arrow_deserialize(v: Option<bool>) -> Option<Self> {
        v
    }
}

impl ArrowDeserialize for NaiveDateTime {
    type ArrayType = PrimitiveArray<i64>;

    #[inline]
    fn arrow_deserialize(v: Option<&i64>) -> Option<Self> {
        v.map(|t| arrow2::temporal_conversions::timestamp_ns_to_datetime(*t))
    }
}

impl ArrowDeserialize for NaiveDate {
    type ArrayType = PrimitiveArray<i32>;

    #[inline]
    fn arrow_deserialize(v: Option<&i32>) -> Option<Self> {
        v.map(|t| arrow2::temporal_conversions::date32_to_date(*t))
    }
}

impl ArrowDeserialize for Vec<u8> {
    type ArrayType = BinaryArray<i32>;

    #[inline]
    fn arrow_deserialize(v: Option<&[u8]>) -> Option<Self> {
        v.map(|t| t.to_vec())
    }
}

impl ArrowDeserialize for LargeBinary {
    type ArrayType = BinaryArray<i64>;

    #[inline]
    fn arrow_deserialize(v: Option<&[u8]>) -> Option<Vec<u8>> {
        v.map(|t| t.to_vec())
    }
}

// Blanket implementation for Vec
impl<T> ArrowDeserialize for Vec<T>
where
    T: ArrowDeserialize + ArrowEnableVecForType + 'static,
    <T as ArrowDeserialize>::ArrayType: 'static,
    for<'b> &'b <T as ArrowDeserialize>::ArrayType: IntoIterator,
{
    type ArrayType = ListArray<i32>;

    fn arrow_deserialize(v: Option<Box<dyn Array>>) -> Option<<Self as ArrowField>::Type> {
        use std::ops::Deref;
        match v {
            Some(t) => arrow_array_deserialize_iterator_internal::<<T as ArrowField>::Type, T>(t.deref())
                .ok()
                .map(|i| i.collect::<Vec<<T as ArrowField>::Type>>()),
            None => None,
        }
    }
}

impl<T> ArrowDeserialize for LargeVec<T>
where
    T: ArrowDeserialize + ArrowEnableVecForType + 'static,
    <T as ArrowDeserialize>::ArrayType: 'static,
    for<'b> &'b <T as ArrowDeserialize>::ArrayType: IntoIterator,
{
    type ArrayType = ListArray<i64>;

    fn arrow_deserialize(v: Option<Box<dyn Array>>) -> Option<<Self as ArrowField>::Type> {
        use std::ops::Deref;
        match v {
            Some(t) => arrow_array_deserialize_iterator_internal::<<T as ArrowField>::Type, T>(t.deref())
                .ok()
                .map(|i| i.collect::<Vec<<T as ArrowField>::Type>>()),
            None => None,
        }
    }
}

impl_arrow_array!(BooleanArray);
impl_arrow_array!(Utf8Array<i32>);
impl_arrow_array!(Utf8Array<i64>);
impl_arrow_array!(BinaryArray<i32>);
impl_arrow_array!(BinaryArray<i64>);
impl_arrow_array!(ListArray<i32>);
impl_arrow_array!(ListArray<i64>);

/// Top-level API to deserialize from Arrow
pub trait TryIntoIter<Collection, Element>
    where Element: ArrowField,
        Collection: FromIterator<Element>
{
    fn try_into_iter(self) -> arrow2::error::Result<Collection>;
    fn try_into_iter_as_type<ArrowType>(self) -> arrow2::error::Result<Collection>
    where ArrowType: ArrowDeserialize + ArrowField<Type = Element> + 'static,
        for<'b> &'b <ArrowType as ArrowDeserialize>::ArrayType: IntoIterator;
}

/// Helper to return an iterator for elements from a [`arrow2::array::Array`].
fn arrow_array_deserialize_iterator_internal<'a, Element, Field>(
    b: &'a dyn arrow2::array::Array,
) -> arrow2::error::Result<impl Iterator<Item = Element> + 'a>
where
    Field: ArrowDeserialize + ArrowField<Type = Element> + 'static,
    for<'b> &'b <Field as ArrowDeserialize>::ArrayType: IntoIterator,
{
    Ok(
        <<Field as ArrowDeserialize>::ArrayType as ArrowArray>::iter_from_array_ref(b)
            .map(<Field as ArrowDeserialize>::arrow_deserialize_internal),
    )
}

pub fn arrow_array_deserialize_iterator_as_type<'a, Element, ArrowType>(
    arr: &'a dyn arrow2::array::Array,
) -> arrow2::error::Result<impl Iterator<Item = Element> + 'a>
where
    Element: 'static,
    ArrowType: ArrowDeserialize + ArrowField<Type = Element> + 'static,
    for<'b> &'b <ArrowType as ArrowDeserialize>::ArrayType: IntoIterator,
{
    if &<ArrowType as ArrowField>::data_type() != arr.data_type() {
        Err(arrow2::error::ArrowError::InvalidArgumentError(
            "Data type mismatch".to_string(),
        ))
    } else {
        Ok(arrow_array_deserialize_iterator_internal::<Element, ArrowType>(arr)?)
    }
}

/// Return an iterator that deserializes an [`Array`] to an element of type T
pub fn arrow_array_deserialize_iterator<'a, T>(
    arr: &'a dyn arrow2::array::Array,
) -> arrow2::error::Result<impl Iterator<Item = T> + 'a>
where
    T: ArrowDeserialize + ArrowField<Type = T> + 'static,
    for<'b> &'b <T as ArrowDeserialize>::ArrayType: IntoIterator,
{
    arrow_array_deserialize_iterator_as_type::<T, T>(arr)
}

impl<'a, Element, ArrowArray> TryIntoIter<Vec<Element>, Element> for ArrowArray
where
    Element: ArrowDeserialize + ArrowField<Type = Element> + 'static,
    for<'b> &'b <Element as ArrowDeserialize>::ArrayType: IntoIterator,
    ArrowArray: std::borrow::Borrow<dyn Array>
{
    fn try_into_iter(self) -> arrow2::error::Result<Vec<Element>> {
        Ok(arrow_array_deserialize_iterator::<Element>(self.borrow())?.collect())
    }

    fn try_into_iter_as_type<ArrowType>(self) -> arrow2::error::Result<Vec<Element>>
    where ArrowType: ArrowDeserialize + ArrowField<Type = Element> + 'static,
        for<'b> &'b <ArrowType as ArrowDeserialize>::ArrayType: IntoIterator
    {
        Ok(arrow_array_deserialize_iterator_as_type::<Element, ArrowType>(self.borrow())?.collect())
    }
}