//! `rkyv::with` adapters for external primitive types (`chrono::NaiveDate` and `rgb::RGB8`).

use chrono::{Datelike, NaiveDate};
use rgb::RGB8;
use std::collections::HashSet;

/// `rkyv::with` wrapper that serializes `chrono::NaiveDate` as a compact `i32` (`num_days_from_ce`).
pub struct RkyvNaiveDate;

impl rkyv::with::ArchiveWith<NaiveDate> for RkyvNaiveDate {
    type Archived = rkyv::rend::i32_le;
    type Resolver = ();

    #[inline]
    fn resolve_with(field: &NaiveDate, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        let days = field.num_days_from_ce();
        rkyv::Archive::resolve(&days, resolver, out);
    }
}

impl<S: rkyv::rancor::Fallible + ?Sized> rkyv::with::SerializeWith<NaiveDate, S> for RkyvNaiveDate {
    #[inline]
    fn serialize_with(field: &NaiveDate, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        let days = field.num_days_from_ce();
        rkyv::Serialize::serialize(&days, serializer)
    }
}

impl<D: rkyv::rancor::Fallible + ?Sized>
    rkyv::with::DeserializeWith<rkyv::rend::i32_le, NaiveDate, D> for RkyvNaiveDate
{
    #[inline]
    fn deserialize_with(
        field: &rkyv::rend::i32_le,
        _deserializer: &mut D,
    ) -> Result<NaiveDate, D::Error> {
        Ok(NaiveDate::from_num_days_from_ce_opt(field.to_native()).unwrap_or_default())
    }
}

/// `rkyv::with` wrapper that serializes `HashSet<NaiveDate>` as a sorted `Vec<i32>` (`num_days_from_ce`).
pub struct RkyvNaiveDateSet;

impl rkyv::with::ArchiveWith<HashSet<NaiveDate>> for RkyvNaiveDateSet {
    type Archived = rkyv::vec::ArchivedVec<rkyv::rend::i32_le>;
    type Resolver = rkyv::vec::VecResolver;

    fn resolve_with(
        field: &HashSet<NaiveDate>,
        resolver: Self::Resolver,
        out: rkyv::Place<Self::Archived>,
    ) {
        let mut days: Vec<i32> = field.iter().map(|d| d.num_days_from_ce()).collect();
        days.sort_unstable();
        rkyv::vec::ArchivedVec::resolve_from_slice(&days, resolver, out);
    }
}

impl<S: rkyv::rancor::Fallible + rkyv::ser::Allocator + rkyv::ser::Writer + ?Sized>
    rkyv::with::SerializeWith<HashSet<NaiveDate>, S> for RkyvNaiveDateSet
{
    fn serialize_with(
        field: &HashSet<NaiveDate>,
        serializer: &mut S,
    ) -> Result<Self::Resolver, S::Error> {
        let mut days: Vec<i32> = field.iter().map(|d| d.num_days_from_ce()).collect();
        days.sort_unstable();
        rkyv::vec::ArchivedVec::serialize_from_slice(&days, serializer)
    }
}

impl<D: rkyv::rancor::Fallible + ?Sized>
    rkyv::with::DeserializeWith<
        rkyv::vec::ArchivedVec<rkyv::rend::i32_le>,
        HashSet<NaiveDate>,
        D,
    > for RkyvNaiveDateSet
{
    fn deserialize_with(
        field: &rkyv::vec::ArchivedVec<rkyv::rend::i32_le>,
        _deserializer: &mut D,
    ) -> Result<HashSet<NaiveDate>, D::Error> {
        Ok(field
            .as_slice()
            .iter()
            .filter_map(|d| NaiveDate::from_num_days_from_ce_opt(d.to_native()))
            .collect())
    }
}

/// `rkyv::with` wrapper that serializes `rgb::RGB8` as `[u8; 3]`.
pub struct RkyvRgb8;

impl rkyv::with::ArchiveWith<RGB8> for RkyvRgb8 {
    type Archived = [u8; 3];
    type Resolver = [(); 3];

    #[inline]
    fn resolve_with(field: &RGB8, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        let arr = [field.r, field.g, field.b];
        rkyv::Archive::resolve(&arr, resolver, out);
    }
}

impl<S: rkyv::rancor::Fallible + ?Sized> rkyv::with::SerializeWith<RGB8, S> for RkyvRgb8 {
    #[inline]
    fn serialize_with(field: &RGB8, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        let arr = [field.r, field.g, field.b];
        rkyv::Serialize::serialize(&arr, serializer)
    }
}

impl<D: rkyv::rancor::Fallible + ?Sized> rkyv::with::DeserializeWith<[u8; 3], RGB8, D>
    for RkyvRgb8
{
    #[inline]
    fn deserialize_with(field: &[u8; 3], _deserializer: &mut D) -> Result<RGB8, D::Error> {
        Ok(RGB8::new(field[0], field[1], field[2]))
    }
}
